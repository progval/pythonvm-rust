pub mod instructions;
pub mod frame;

use super::objects::{Code, ObjectStore, ObjectRef, ObjectContent, PrimitiveObjects, Object};
use super::sandbox::EnvProxy;
use super::varstack::{VarStack, VectorVarStack};
use self::instructions::{CmpOperator, Instruction};
use self::frame::{Block, Frame};
use std::fmt;
use std::collections::HashMap;
use std::io::Read;
use std::rc::Rc;
use std::cell::RefCell;
use super::marshal;

#[derive(Debug)]
pub enum ProcessorError {
    CircularReference,
    InvalidReference,
    NotACodeObject(String),
    NotAFunctionObject(String),
    CodeObjectIsNotBytes,
    InvalidProgramCounter,
    StackTooSmall,
    InvalidConstIndex,
    InvalidName(String),
    InvalidNameIndex,
    InvalidVarnameIndex,
    UnknownPrimitive(String),
    UnmarshalError(marshal::decode::UnmarshalError),
    InvalidModuleName(String),
}

impl fmt::Display for ProcessorError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
       fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug)]
pub enum PyResult {
    Return(ObjectRef),
    Raise(ObjectRef, ObjectRef), // (exception, exc_type)
    Error(ProcessorError),
}


pub type PyFunction<EP> = fn(&mut Processor<EP>, Vec<ObjectRef>) -> PyResult;


/// Like try!, but for PyResult instead of Result
macro_rules! py_try {
    ( $py_res: expr ) => {
        match $py_res {
            PyResult::Return(res) => res,
            py_res => return py_res
        }
    };
    ( $rust_res: expr, $rust_err_to_py_err: expr ) => {
        match $rust_res {
            Ok(res) => res,
            Err(err) => return PyResult::Error($rust_err_to_py_err(err)),
        }
    }
}

/// Like Option::unwrap, but returns a PyResult::Error instead of panic.
macro_rules! py_unwrap {
    ( $rust_res: expr, $err: expr ) => {
        match $rust_res {
            Some(res) => res,
            None => return PyResult::Error($err),
        }
    }
}

macro_rules! pop_stack {
    ( $stack_name:expr) => {
        py_unwrap!($stack_name.pop(), ProcessorError::StackTooSmall)
    }
}

macro_rules! raise {
    ($call_stack: expr, $traceback: expr, $exception: expr, $exc_type: expr, $value: expr) => {{
        let frame = $call_stack.last_mut().unwrap();
        loop {
            match frame.block_stack.pop() {
                None => { // Root block; raise exception to calling function
                    return PyResult::Raise($exception, $exc_type)
                }
                Some(Block::TryExcept(begin, end)) => { // Found a try…except block
                    frame.block_stack.push(Block::TryExcept(begin, end)); // Push it back, it will be poped by PopExcept.
                    frame.program_counter = end;
                    let traceback = $traceback;
                    let exception = $exception;
                    frame.var_stack.push(traceback.clone()); // traceback
                    frame.var_stack.push(exception.clone()); // exception
                    frame.var_stack.push($exc_type); // exception type

                    frame.var_stack.push(traceback); // traceback
                    frame.var_stack.push($value); // value
                    frame.var_stack.push(exception); // exception
                    break
                }
                Some(_) => { // Non-try…except block, exit it.
                }
            }
        }
    }}
}

pub struct Processor<EP: EnvProxy> {
    pub envproxy: EP,
    pub store: ObjectStore,
    pub primitive_functions: HashMap<String, PyFunction<EP>>,
    pub primitive_objects: PrimitiveObjects,
    pub modules: HashMap<String, Rc<RefCell<HashMap<String, ObjectRef>>>>,
}

impl<EP: EnvProxy> Processor<EP> {

    // Load a name from the namespace
    fn load_name(&mut self, frame: &Frame, name: &String) -> PyResult {
        if *name == "__primitives__" {
            return PyResult::Return(self.store.allocate(Object { name: Some("__primitives__".to_string()), content: ObjectContent::PrimitiveNamespace, class: self.primitive_objects.object.clone(), bases: None }))
        }
        if *name == "__name__" {
            return PyResult::Return(self.store.allocate(self.primitive_objects.new_string("<module>".to_string())))
        }
        if let Some(obj_ref) = frame.locals.borrow().get(name) {
            return PyResult::Return(obj_ref.clone())
        }
        if let Some(m) = self.modules.get("builtins") {
            if let Some(obj_ref) = m.borrow().get(name) {
                return PyResult::Return(obj_ref.clone())
            }
        }
        if let Some(m) = self.modules.get(&frame.object.module(&self.store)) {
            if let Some(obj_ref) = m.borrow().get(name) {
                return PyResult::Return(obj_ref.clone())
            }
        }
        panic!(format!("Cannot load {}: neither in __primitives__, locals, nor globals.", name))
    }


    fn load_attr(&mut self, obj: &Object, name: String) -> PyResult {
        match name.as_ref() {
            "__bases__" => {
                match obj.bases {
                    Some(ref v) => PyResult::Return(self.store.allocate(self.primitive_objects.new_tuple(v.clone()))),
                    None => PyResult::Return(self.primitive_objects.none.clone()),
                }
            },
            "__name__" => {
                match obj.name {
                    Some(ref s) => PyResult::Return(self.store.allocate(self.primitive_objects.new_string(s.clone()))),
                    None => panic!("No __name__"),
                }
            },
            _ => {
                if let ObjectContent::PrimitiveNamespace = obj.content {
                    match self.primitive_objects.names_map.get(&name) {
                        Some(obj_ref) => PyResult::Return(obj_ref.clone()),
                        None => PyResult::Return(self.store.allocate(Object { name: Some(name.clone()), content: ObjectContent::PrimitiveFunction(name), class: self.primitive_objects.function_type.clone(), bases: None })),
                    }
                }
                else {
                    panic!(format!("Not implemented: looking up attribute '{}' of {:?}", name, obj))
                }
            }
        }
    }

    // Call a primitive / function / code object, with arguments.
    fn call_function(&mut self, call_stack: &mut Vec<Frame>, func_ref: &ObjectRef, mut args: Vec<ObjectRef>, kwargs: Vec<ObjectRef>) -> PyResult {
        // TODO: clone only if necessary
        match self.store.deref(func_ref).content.clone() {
            ObjectContent::Class(None) => {
                PyResult::Return(self.store.allocate(Object::new_instance(None, func_ref.clone(), ObjectContent::OtherObject)))
            },
            ObjectContent::Class(Some(ref code_ref)) => {
                // TODO: run code
                PyResult::Return(self.store.allocate(Object::new_instance(None, func_ref.clone(), ObjectContent::OtherObject)))
            },
            ObjectContent::Function(ref func_module, ref code_ref) => {
                let code = self.store.deref(code_ref).content.clone();
                if let ObjectContent::Code(code) = code {
                    if code.co_varargs() { // If it has a *args argument
                        if code.argcount > args.len() {
                            panic!(format!("Function {} expected at least {} arguments, but got {}.", code.name, code.argcount, args.len()))
                        };
                        let to_vararg = args.drain(code.argcount..).collect();
                        let obj_ref = self.store.allocate(self.primitive_objects.new_tuple(to_vararg));
                        args.push(obj_ref);
                    }
                    else if code.argcount != args.len() {
                        panic!(format!("Function {} expected {} arguments, but got {}.", code.name, code.argcount, args.len()))
                    };
                    let mut locals = Rc::new(RefCell::new(HashMap::new()));
                    {
                        let mut locals = locals.borrow_mut();
                        for (argname, argvalue) in code.varnames.iter().zip(args) {
                            locals.insert(argname.clone(), argvalue);
                        };
                    }
                    let new_frame = Frame::new(func_ref.clone(), locals.clone());
                    call_stack.push(new_frame);
                    let res = self.run_code(call_stack, (*code).clone());
                    call_stack.pop();
                    res
                }
                else {
                    return PyResult::Error(ProcessorError::NotACodeObject(func_ref.repr(&self.store)))
                }
            },
            ObjectContent::PrimitiveFunction(ref name) => {
                let function = match self.primitive_functions.get(name) {
                    Some(function) => function.clone(),
                    None => return PyResult::Error(ProcessorError::UnknownPrimitive(name.clone())),
                };
                function(self, args)
            },
            _ => {
                return PyResult::Error(ProcessorError::NotAFunctionObject(format!("called {:?}", self.store.deref(func_ref))));
            }
        }
    }

    // Main interpreter loop
    // See https://docs.python.org/3/library/dis.html for a description of instructions
    fn run_code(&mut self, call_stack: &mut Vec<Frame>, code: Code) -> PyResult {
        let bytecode: Vec<u8> = code.code;
        let instructions: Vec<Instruction> = instructions::InstructionDecoder::new(bytecode.iter()).into_iter().collect();
        loop {
            let instruction = {
                let frame = call_stack.last_mut().unwrap();
                let instruction = py_unwrap!(instructions.get(frame.program_counter), ProcessorError::InvalidProgramCounter);
                frame.program_counter += 1;
                instruction
            };
            match *instruction {
                Instruction::PopTop => {
                    let frame = call_stack.last_mut().unwrap();
                    pop_stack!(frame.var_stack);
                    ()
                },
                Instruction::DupTop => {
                    let frame = call_stack.last_mut().unwrap();
                    let val = pop_stack!(frame.var_stack);
                    frame.var_stack.push(val.clone());
                    frame.var_stack.push(val);
                }
                Instruction::Nop => (),
                Instruction::BinarySubscr => {
                    let frame = call_stack.last_mut().unwrap();
                    let index_ref = pop_stack!(frame.var_stack);
                    let index = self.store.deref(&index_ref).content.clone();
                    let container_ref = pop_stack!(frame.var_stack);
                    let container = self.store.deref(&container_ref).content.clone();
                    match (container, index) {
                        (ObjectContent::Tuple(v), ObjectContent::Int(i)) | (ObjectContent::List(v), ObjectContent::Int(i)) => {
                            match v.get(i as usize) { // TODO: overflow check
                                None => panic!("Out of bound"),
                                Some(obj_ref) => frame.var_stack.push(obj_ref.clone()),
                            }
                        }
                        _ => panic!("Indexing only supported for tuples/lists with an integer.")
                    }
                }
                Instruction::LoadBuildClass => {
                    let frame = call_stack.last_mut().unwrap();
                    let obj = Object { name: Some("__build_class__".to_string()), content: ObjectContent::PrimitiveFunction("build_class".to_string()), class: self.primitive_objects.function_type.clone(), bases: None };
                    frame.var_stack.push(self.store.allocate(obj));
                }
                Instruction::ReturnValue => {
                    let frame = call_stack.last_mut().unwrap();
                    return PyResult::Return(pop_stack!(frame.var_stack))
                }
                Instruction::PopBlock => {
                    let frame = call_stack.last_mut().unwrap();
                    pop_stack!(frame.block_stack);
                }
                Instruction::EndFinally => {
                    let frame = call_stack.last_mut().unwrap();
                    let status_ref = pop_stack!(frame.var_stack);
                    let status = self.store.deref(&status_ref);
                    match status.content {
                        ObjectContent::Int(i) => panic!("TODO: finally int status"), // TODO
                        ObjectContent::OtherObject => {}
                        _ => panic!("Invalid finally status")
                    }
                }
                Instruction::PopExcept => {
                    let frame = call_stack.last_mut().unwrap();
                    let mut three_last = frame.var_stack.pop_all_and_get_n_last(3).unwrap(); // TODO: check
                    let exc_type = three_last.pop();
                    let exc_value = three_last.pop();
                    let exc_traceback = three_last.pop();
                    // TODO: do something with exc_*
                    pop_stack!(frame.block_stack);
                },
                Instruction::StoreName(i) => {
                    let frame = call_stack.last_mut().unwrap();
                    let name = py_unwrap!(code.names.get(i), ProcessorError::InvalidNameIndex).clone();
                    let obj_ref = pop_stack!(frame.var_stack);
                    frame.locals.borrow_mut().insert(name, obj_ref);
                }
                Instruction::LoadConst(i) => {
                    let frame = call_stack.last_mut().unwrap();
                    frame.var_stack.push(py_unwrap!(code.consts.get(i), ProcessorError::InvalidConstIndex).clone())
                }
                Instruction::LoadName(i) | Instruction::LoadGlobal(i) => { // TODO: LoadGlobal should look only in globals
                    let frame = call_stack.last_mut().unwrap();
                    let name = py_unwrap!(code.names.get(i), ProcessorError::InvalidNameIndex);
                    let obj_ref = py_try!(self.load_name(&frame, name));
                    frame.var_stack.push(obj_ref)
                }
                Instruction::LoadAttr(i) => {
                    let frame = call_stack.last_mut().unwrap();
                    let obj = {
                        let obj_ref = py_unwrap!(frame.var_stack.pop(), ProcessorError::StackTooSmall);
                        self.store.deref(&obj_ref).clone()
                    };
                    let name = py_unwrap!(code.names.get(i), ProcessorError::InvalidNameIndex).clone();
                    frame.var_stack.push(py_try!(self.load_attr(&obj, name)))
                },
                Instruction::SetupLoop(i) => {
                    let frame = call_stack.last_mut().unwrap();
                    frame.block_stack.push(Block::Loop(frame.program_counter, frame.program_counter+i))
                }
                Instruction::SetupExcept(i) => {
                    let frame = call_stack.last_mut().unwrap();
                    frame.block_stack.push(Block::TryExcept(frame.program_counter, frame.program_counter+i))
                }
                Instruction::CompareOp(CmpOperator::Eq) => {
                    let frame = call_stack.last_mut().unwrap();
                    // TODO: enrich this (support __eq__)
                    let obj1 = self.store.deref(&pop_stack!(frame.var_stack));
                    let obj2 = self.store.deref(&pop_stack!(frame.var_stack));
                    if obj1.name == obj2.name && obj1.content == obj2.content {
                        frame.var_stack.push(self.primitive_objects.true_obj.clone())
                    }
                    else {
                        frame.var_stack.push(self.primitive_objects.false_obj.clone())
                    };
                }
                Instruction::CompareOp(CmpOperator::ExceptionMatch) => {
                    let frame = call_stack.last_mut().unwrap();
                    // TODO: add support for tuples
                    let pattern_ref = pop_stack!(frame.var_stack);
                    let exc_ref = pop_stack!(frame.var_stack);
                    let isinstance = self.primitive_functions.get("isinstance").unwrap().clone();
                    frame.var_stack.push(py_try!(isinstance(self, vec![exc_ref, pattern_ref])));
                }
                Instruction::JumpForward(delta) => {
                    let frame = call_stack.last_mut().unwrap();
                    frame.program_counter += delta
                }
                Instruction::LoadFast(i) => {
                    let frame = call_stack.last_mut().unwrap();
                    let name = py_unwrap!(code.varnames.get(i), ProcessorError::InvalidVarnameIndex).clone();
                    let obj_ref = py_unwrap!(frame.locals.borrow().get(&name), ProcessorError::InvalidName(name)).clone();
                    frame.var_stack.push(obj_ref)
                }
                Instruction::PopJumpIfFalse(target) => {
                    let frame = call_stack.last_mut().unwrap();
                    let obj = self.store.deref(&pop_stack!(frame.var_stack));
                    match obj.content {
                        ObjectContent::True => (),
                        ObjectContent::False => frame.program_counter = target,
                        _ => unimplemented!(),
                    }
                }

                Instruction::RaiseVarargs(0) => {
                    panic!("RaiseVarargs(0) not implemented.")
                }
                Instruction::RaiseVarargs(1) => {
                    let exception = pop_stack!(call_stack.last_mut().unwrap().var_stack);
                    let exc_type = exception.clone();
                    raise!(call_stack, self.primitive_objects.none.clone(), exception, exc_type, self.primitive_objects.none.clone());
                }
                Instruction::RaiseVarargs(2) => {
                    panic!("RaiseVarargs(2) not implemented.")
                }
                Instruction::RaiseVarargs(_) => {
                    // Note: the doc lies, the argument can only be ≤ 2
                    panic!("Bad RaiseVarargs argument") // TODO: Raise an exception instead
                }

                Instruction::CallFunction(nb_args, nb_kwargs) => {
                    // See “Call constructs” at:
                    // http://security.coverity.com/blog/2014/Nov/understanding-python-bytecode.html
                    let mut kwargs;
                    let mut args;
                    let mut func;
                    {
                        let frame = call_stack.last_mut().unwrap();
                        kwargs = py_unwrap!(frame.var_stack.pop_many(nb_kwargs*2), ProcessorError::StackTooSmall);
                        args = py_unwrap!(frame.var_stack.pop_many(nb_args), ProcessorError::StackTooSmall);
                        func = pop_stack!(frame.var_stack);
                    }
                    let ret = self.call_function(call_stack, &func, args, kwargs);
                    match ret {
                        PyResult::Return(obj_ref) => call_stack.last_mut().unwrap().var_stack.push(obj_ref),
                        PyResult::Raise(exception, exc_type) => {
                            raise!(call_stack, self.primitive_objects.none.clone(), exception, exc_type, self.primitive_objects.none.clone())
                        },
                        PyResult::Error(err) => return PyResult::Error(err)
                    };
                },
                Instruction::MakeFunction(0, 0, 0) => {
                    // TODO: consume default arguments and annotations
                    let frame = call_stack.last_mut().unwrap();
                    let func_name = match self.store.deref(&pop_stack!(frame.var_stack)).content {
                        ObjectContent::String(ref s) => s.clone(),
                        _ => panic!("Function names must be strings."),
                    };
                    let code = pop_stack!(frame.var_stack);
                    let func = self.primitive_objects.new_function(func_name, frame.object.module(&self.store), code);
                    frame.var_stack.push(self.store.allocate(func))
                },
                _ => panic!(format!("todo: instruction {:?}", *instruction)),
            }
        };
    }

    fn call_module_code(&mut self, call_stack: &mut Vec<Frame>, module_name: String, module_ref: ObjectRef) -> PyResult {
        let code_ref = match self.store.deref(&module_ref).content {
            ObjectContent::Module(ref code_ref) => code_ref.clone(),
            ref o => panic!("Not a module: {:?}", o),
        };
        let code = match self.store.deref(&code_ref).content {
            ObjectContent::Code(ref code) => code.clone(),
            ref o => return PyResult::Error(ProcessorError::NotACodeObject(format!("file code {:?}", o))),
        };
        let module_obj = self.store.allocate(self.primitive_objects.new_module(module_name.clone(), code_ref));
        self.modules.insert(module_name.clone(), Rc::new(RefCell::new(HashMap::new())));
        let mut call_stack = vec![Frame::new(module_obj, self.modules.get(&module_name).unwrap().clone())];
        let res = self.run_code(&mut call_stack, *code);
        let frame = call_stack.pop().unwrap();
        res // Do not raise exceptions before the pop()
    }

    /// Get the code of a module from its name
    pub fn get_module_code(&mut self, call_stack: &mut Vec<Frame>, module_name: String) -> PyResult {
        // Load the code
        let mut module_bytecode = self.envproxy.open_module(module_name.clone());
        let mut buf = [0; 12];
        module_bytecode.read_exact(&mut buf).unwrap();
        if !marshal::check_magic(&buf[0..4]) {
            panic!(format!("Bad magic number for module {}.", module_name))
        }
        let module_code_ref = py_try!(marshal::read_object(&mut module_bytecode, &mut self.store, &self.primitive_objects), ProcessorError::UnmarshalError);
        let module_code = match self.store.deref(&module_code_ref).content {
            ObjectContent::Code(ref code) => code.clone(),
            ref o => return PyResult::Error(ProcessorError::NotACodeObject(format!("module code {:?}", o))),
        };
        PyResult::Return(self.store.allocate(self.primitive_objects.new_module(module_name.clone(), module_code_ref)))
    }

    /// Entry point to run code. Loads builtins in the code's namespace and then run it.
    pub fn call_main_code(&mut self, code_ref: ObjectRef) -> PyResult {
        let mut call_stack = Vec::new();
        let builtins_code_ref = py_try!(self.get_module_code(&mut call_stack, "builtins".to_string()));
        py_try!(self.call_module_code(&mut call_stack, "builtins".to_string(), builtins_code_ref));

        let mut call_stack = Vec::new();
        let module_ref = self.store.allocate(self.primitive_objects.new_module("__main__".to_string(), code_ref));
        self.call_module_code(&mut call_stack, "__main__".to_string(), module_ref)
    }
}
