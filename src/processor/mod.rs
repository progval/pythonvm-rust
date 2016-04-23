pub mod instructions;

use super::objects::{Code, ObjectStore, ObjectRef, ObjectContent, PrimitiveObjects, Object};
use super::sandbox::EnvProxy;
use super::stack::{Stack, VectorStack};
use self::instructions::{CmpOperator, Instruction};
use std::fmt;
use std::collections::HashMap;
use std::io::Read;
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
    InvalidNameIndex,
    InvalidVarnameIndex,
    UnknownPrimitive(String),
    UnmarshalError(marshal::decode::UnmarshalError),
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

#[derive(Debug)]
enum Block {
    Loop(usize, usize), // begin, end
    TryExcept(usize, usize), // begin, end
}

struct Stacks {
    variables: VectorStack<ObjectRef>,
    blocks: VectorStack<Block>,
}

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
    ($stacks: expr, $program_counter: ident, $traceback: expr, $exception: expr, $exc_type: expr, $value: expr) => {{
        loop {
            match $stacks.blocks.pop() {
                None => { // Root block; raise exception to calling function
                    return PyResult::Raise($exception, $exc_type)
                }
                Some(Block::TryExcept(begin, end)) => { // Found a try…except block
                    $stacks.blocks.push(Block::TryExcept(begin, end)); // Push it back, it will be poped by PopExcept.
                    $program_counter = end;
                    let traceback = $traceback;
                    let exception = $exception;
                    $stacks.variables.push(traceback.clone()); // traceback
                    $stacks.variables.push(exception.clone()); // exception
                    $stacks.variables.push($exc_type); // exception type

                    $stacks.variables.push(traceback); // traceback
                    $stacks.variables.push($value); // value
                    $stacks.variables.push(exception); // exception
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
    pub primitive_objects: PrimitiveObjects
}

impl<EP: EnvProxy> Processor<EP> {

    // Load a name from the namespace (only __primitive__ and locals for now)
    fn load_name(&mut self, namespace: &mut HashMap<String, ObjectRef>, name: String) -> PyResult {
        if name == "__primitives__" {
            PyResult::Return(self.store.allocate(Object { name: Some("__primitives__".to_string()), content: ObjectContent::PrimitiveNamespace, class: self.primitive_objects.object.clone(), bases: None }))
        }
        else if name == "__name__" {
            PyResult::Return(self.store.allocate(self.primitive_objects.new_string("<module>".to_string())))
        }
        else if let Some(obj_ref) = namespace.get(&name) {
            PyResult::Return(obj_ref.clone())
        }
        else {
            panic!(format!("Cannot load {}: neither __primitives__ or in namespace.", name))
        }
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
    fn call_function(&mut self, namespace: &mut HashMap<String, ObjectRef>, func_ref: &ObjectRef, mut args: Vec<ObjectRef>, kwargs: Vec<ObjectRef>) -> PyResult {
        // TODO: clone only if necessary
        match self.store.deref(func_ref).content.clone() {
            ObjectContent::Class(None) => {
                PyResult::Return(self.store.allocate(Object::new_instance(None, func_ref.clone(), ObjectContent::OtherObject)))
            },
            ObjectContent::Class(Some(ref code_ref)) => {
                // TODO: run code
                PyResult::Return(self.store.allocate(Object::new_instance(None, func_ref.clone(), ObjectContent::OtherObject)))
            },
            ObjectContent::Function(ref code_ref) => {
                let code = self.store.deref(code_ref).content.clone();
                if let ObjectContent::Code(code) = code {
                        let mut namespace = namespace.clone(); // TODO: costly, try maybe copy-on-write?
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
                    for (argname, argvalue) in code.varnames.iter().zip(args) {
                        namespace.insert(argname.clone(), argvalue);
                    };
                    self.run_code(&mut namespace, (*code).clone())
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
    fn run_code(&mut self, namespace: &mut HashMap<String, ObjectRef>, code: Code) -> PyResult {
        let bytecode: Vec<u8> = code.code;
        let instructions: Vec<Instruction> = instructions::InstructionDecoder::new(bytecode.iter()).into_iter().collect();
        let mut program_counter = 0 as usize;
        let mut stacks = Stacks { variables: VectorStack::new(), blocks: VectorStack::new() };
        loop {
            let instruction = py_unwrap!(instructions.get(program_counter), ProcessorError::InvalidProgramCounter);
            program_counter += 1;
            match *instruction {
                Instruction::PopTop => {
                    pop_stack!(stacks.variables);
                    ()
                },
                Instruction::DupTop => {
                    let val = pop_stack!(stacks.variables);
                    stacks.variables.push(val.clone());
                    stacks.variables.push(val);
                }
                Instruction::Nop => (),
                Instruction::BinarySubscr => {
                    let index_ref = pop_stack!(stacks.variables);
                    let index = self.store.deref(&index_ref).content.clone();
                    let container_ref = pop_stack!(stacks.variables);
                    let container = self.store.deref(&container_ref).content.clone();
                    match (container, index) {
                        (ObjectContent::Tuple(v), ObjectContent::Int(i)) | (ObjectContent::List(v), ObjectContent::Int(i)) => {
                            match v.get(i as usize) { // TODO: overflow check
                                None => panic!("Out of bound"),
                                Some(obj_ref) => stacks.variables.push(obj_ref.clone()),
                            }
                        }
                        _ => panic!("Indexing only supported for tuples/lists with an integer.")
                    }
                }
                Instruction::LoadBuildClass => {
                    let obj = Object { name: Some("__build_class__".to_string()), content: ObjectContent::PrimitiveFunction("build_class".to_string()), class: self.primitive_objects.function_type.clone(), bases: None };
                    stacks.variables.push(self.store.allocate(obj));
                }
                Instruction::ReturnValue => return PyResult::Return(pop_stack!(stacks.variables)),
                Instruction::PopBlock => { pop_stack!(stacks.blocks); },
                Instruction::EndFinally => {
                    let status_ref = pop_stack!(stacks.variables);
                    let status = self.store.deref(&status_ref);
                    match status.content {
                        ObjectContent::Int(i) => panic!("TODO: finally int status"), // TODO
                        ObjectContent::OtherObject => {}
                        _ => panic!("Invalid finally status")
                    }
                }
                Instruction::PopExcept => {
                    let mut three_last = stacks.variables.pop_all_and_get_n_last(3).unwrap(); // TODO: check
                    let exc_type = three_last.pop();
                    let exc_value = three_last.pop();
                    let exc_traceback = three_last.pop();
                    // TODO: do something with exc_*
                    pop_stack!(stacks.blocks);
                },
                Instruction::StoreName(i) => {
                    let name = py_unwrap!(code.names.get(i), ProcessorError::InvalidNameIndex).clone();
                    let obj_ref = pop_stack!(stacks.variables);
                    namespace.insert(name, obj_ref);
                }
                Instruction::LoadConst(i) => stacks.variables.push(py_unwrap!(code.consts.get(i), ProcessorError::InvalidConstIndex).clone()),
                Instruction::LoadName(i) | Instruction::LoadGlobal(i) => {
                    let name = py_unwrap!(code.names.get(i), ProcessorError::InvalidNameIndex).clone();
                    stacks.variables.push(py_try!(self.load_name(namespace, name)))
                }
                Instruction::LoadAttr(i) => {
                    let obj = {
                        let obj_ref = py_unwrap!(stacks.variables.pop(), ProcessorError::StackTooSmall);
                        self.store.deref(&obj_ref).clone()
                    };
                    let name = py_unwrap!(code.names.get(i), ProcessorError::InvalidNameIndex).clone();
                    stacks.variables.push(py_try!(self.load_attr(&obj, name)))
                },
                Instruction::SetupLoop(i) => {
                    stacks.blocks.push(Block::Loop(program_counter, program_counter+i))
                }
                Instruction::SetupExcept(i) => {
                    stacks.blocks.push(Block::TryExcept(program_counter, program_counter+i))
                }
                Instruction::CompareOp(CmpOperator::Eq) => {
                    // TODO: enrich this (support __eq__)
                    let obj1 = self.store.deref(&pop_stack!(stacks.variables));
                    let obj2 = self.store.deref(&pop_stack!(stacks.variables));
                    if obj1.name == obj2.name && obj1.content == obj2.content {
                        stacks.variables.push(self.primitive_objects.true_obj.clone())
                    }
                    else {
                        stacks.variables.push(self.primitive_objects.false_obj.clone())
                    };
                }
                Instruction::CompareOp(CmpOperator::ExceptionMatch) => {
                    // TODO: add support for tuples
                    let pattern_ref = pop_stack!(stacks.variables);
                    let exc_ref = pop_stack!(stacks.variables);
                    let isinstance = self.primitive_functions.get("isinstance").unwrap().clone();
                    stacks.variables.push(py_try!(isinstance(self, vec![exc_ref, pattern_ref])));
                }
                Instruction::JumpForward(delta) => {
                    program_counter += delta
                }
                Instruction::LoadFast(i) => {
                    let name = py_unwrap!(code.varnames.get(i), ProcessorError::InvalidVarnameIndex).clone();
                    stacks.variables.push(py_try!(self.load_name(namespace, name)))
                }
                Instruction::PopJumpIfFalse(target) => {
                    let obj = self.store.deref(&pop_stack!(stacks.variables));
                    match obj.content {
                        ObjectContent::True => (),
                        ObjectContent::False => program_counter = target,
                        _ => unimplemented!(),
                    }
                }

                Instruction::RaiseVarargs(0) => {
                    panic!("RaiseVarargs(0) not implemented.")
                }
                Instruction::RaiseVarargs(1) => {
                    let exception = pop_stack!(stacks.variables);
                    let exc_type = exception.clone();
                    // TODO: add traceback
                    raise!(stacks, program_counter, self.primitive_objects.none.clone(), exception, exc_type, self.primitive_objects.none.clone());
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
                    let kwargs = py_unwrap!(stacks.variables.pop_many(nb_kwargs*2), ProcessorError::StackTooSmall);
                    let args = py_unwrap!(stacks.variables.pop_many(nb_args), ProcessorError::StackTooSmall);
                    let func = pop_stack!(stacks.variables);
                    let ret = self.call_function(namespace, &func, args, kwargs);
                    match ret {
                        PyResult::Return(obj_ref) => stacks.variables.push(obj_ref),
                        PyResult::Raise(exception, exc_type) => {
                            // TODO: add frame to traceback
                            raise!(stacks, program_counter, self.primitive_objects.none.clone(), exception, exc_type, self.primitive_objects.none.clone())
                        },
                        PyResult::Error(err) => return PyResult::Error(err)
                    };
                },
                Instruction::MakeFunction(0, 0, 0) => {
                    // TODO: consume default arguments and annotations
                    let func_name = match self.store.deref(&pop_stack!(stacks.variables)).content {
                        ObjectContent::String(ref s) => s.clone(),
                        _ => panic!("Function names must be strings."),
                    };
                    let code = pop_stack!(stacks.variables);
                    stacks.variables.push(self.store.allocate(Object { name: Some(func_name), content: ObjectContent::Function(code), class: self.primitive_objects.function_type.clone(), bases: None }))
                },
                _ => panic!(format!("todo: instruction {:?}", *instruction)),
            }
        };
    }

    /// Load a module from its name and run it.
    /// Functions and attributes will be added in the `namespace`.
    pub fn run_module(&mut self, namespace: &mut HashMap<String, ObjectRef>, module_name: String) -> PyResult {
        let mut builtins_bytecode = self.envproxy.open_module(module_name);
        let mut buf = [0; 12];
        builtins_bytecode.read_exact(&mut buf).unwrap();
        if !marshal::check_magic(&buf[0..4]) {
            panic!("Bad magic number for builtins.py.")
        }
        let builtins_code = py_try!(marshal::read_object(&mut builtins_bytecode, &mut self.store, &self.primitive_objects), ProcessorError::UnmarshalError);
        let builtins_code = match self.store.deref(&builtins_code).content {
            ObjectContent::Code(ref code) => code.clone(),
            ref o => return PyResult::Error(ProcessorError::NotACodeObject(format!("builtins code {:?}", o))),
        };
        self.run_code(namespace, *builtins_code)
    }

    /// Entry point to run code. Loads builtins in the code's namespace and then run it.
    pub fn run_code_object(&mut self, code_object: ObjectRef) -> PyResult {
        let mut builtins = HashMap::new();
        py_try!(self.run_module(&mut builtins, "builtins".to_string()));

        let code = match self.store.deref(&code_object).content {
            ObjectContent::Code(ref code) => code.clone(),
            ref o => return PyResult::Error(ProcessorError::NotACodeObject(format!("file code {:?}", o))),
        };
        self.run_code(&mut builtins, *code)
    }
}
