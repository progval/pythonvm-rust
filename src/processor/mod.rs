pub mod instructions;
pub mod frame;

use super::objects::{Code, ObjectStore, ObjectRef, ObjectContent, PrimitiveObjects, Object};
use super::varstack::{VarStack, VectorVarStack};
use self::instructions::{CmpOperator, Instruction};
use self::frame::{Block, Frame};
use std::fmt;
use std::collections::HashMap;
use std::io::Read;
use std::cell::RefCell;
use std::rc::Rc;
use super::marshal;
use super::state::{State, PyResult};
use super::sandbox::EnvProxy;

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


/// Like try!, but for PyResult instead of Result
macro_rules! py_try {
    ( $py_res: expr ) => {
        match $py_res {
            PyResult::Return(res) => res,
            py_res => return py_res
        }
    };
}


/// Like Option::unwrap, but raises an exception instead of panic.
macro_rules! py_unwrap {
    ( $state: expr, $rust_res: expr, $err: expr ) => {
        match $rust_res {
            Some(res) => res,
            None => return $state.raise_processor_error($err),
        }
    }
}

macro_rules! pop_stack {
    ( $state: expr, $stack_name: expr) => {
        py_unwrap!($state, $stack_name.pop(), ProcessorError::StackTooSmall)
    }
}

/// Unwind call stack until a try…except is found.
fn unwind(call_stack: &mut Vec<Frame>, traceback: ObjectRef, exception: ObjectRef, value: ObjectRef) {
    let exc_type = exception.clone(); // Looks like that's how CPython does things…
    'outer: loop {
        match call_stack.pop() {
            None => panic!("Exception reached bottom of call stack."),
            Some(mut frame) => {
                // Unwind block stack
                while let Some(block) = frame.block_stack.pop() {
                    match block {
                        Block::TryExcept(begin, end) => {
                            // Found a try…except block
                            frame.block_stack.push(Block::TryExcept(begin, end)); // Push it back, it will be poped by PopExcept.
                            frame.program_counter = end;
                            frame.var_stack.push(traceback.clone());
                            frame.var_stack.push(value.clone());
                            frame.var_stack.push(exc_type);

                            frame.var_stack.push(traceback);
                            frame.var_stack.push(value);
                            frame.var_stack.push(exception);

                            call_stack.push(frame);
                            break 'outer
                        }
                        _ => { // Non-try…except block, exit it.
                        }
                    }
                }
            }
        }
    }
}

macro_rules! raise {
    ($state: ident, $call_stack: expr, $exc_class: expr, $msg: expr) => {{
        let exc = Object::new_instance(None, $exc_class.clone(), ObjectContent::String($msg));
        let exc = $state.store.allocate(exc);
        // TODO: actual traceback
        unwind($call_stack, $state.primitive_objects.none.clone(), exc, $state.primitive_objects.none.clone())
    }}
}


// Load a name from the namespace
fn load_name<EP: EnvProxy>(state: &mut State<EP>, frame: &Frame, name: &String) -> Option<ObjectRef> {
    if *name == "__primitives__" {
        return Some(state.store.allocate(Object { name: Some("__primitives__".to_string()), content: ObjectContent::PrimitiveNamespace, class: state.primitive_objects.object.clone(), bases: None }))
    }
    if *name == "__name__" {
        return Some(state.store.allocate(state.primitive_objects.new_string("<module>".to_string())))
    }
    if let Some(obj_ref) = frame.locals.borrow().get(name) {
        return Some(obj_ref.clone())
    }
    if let Some(m) = state.modules.get("builtins") {
        if let Some(obj_ref) = m.borrow().get(name) {
            return Some(obj_ref.clone())
        }
    }
    if let Some(m) = state.modules.get(&frame.object.module(&state.store)) {
        if let Some(obj_ref) = m.borrow().get(name) {
            return Some(obj_ref.clone())
        }
    }
    None
}


fn load_attr<EP: EnvProxy>(state: &mut State<EP>, obj: &Object, name: &String) -> Option<ObjectRef> {
    match name.as_ref() {
        "__bases__" => {
            match obj.bases {
                Some(ref v) => Some(state.store.allocate(state.primitive_objects.new_tuple(v.clone()))),
                None => Some(state.primitive_objects.none.clone()),
            }
        },
        "__name__" => {
            match obj.name {
                Some(ref s) => Some(state.store.allocate(state.primitive_objects.new_string(s.clone()))),
                None => None,
            }
        },
        _ => {
            if let ObjectContent::PrimitiveNamespace = obj.content {
                match state.primitive_objects.names_map.get(name) {
                    Some(obj_ref) => Some(obj_ref.clone()),
                    None => Some(state.store.allocate(Object { name: Some(name.clone()), content: ObjectContent::PrimitiveFunction(name.clone()), class: state.primitive_objects.function_type.clone(), bases: None })),
                }
            }
            else {
                panic!(format!("Not implemented: looking up attribute '{}' of {:?}", name, obj))
            }
        }
    }
}

// Call a primitive / function / code object, with arguments.
fn call_function<EP: EnvProxy>(state: &mut State<EP>, call_stack: &mut Vec<Frame>, func_ref: &ObjectRef, mut args: Vec<ObjectRef>, kwargs: Vec<ObjectRef>) {
    // TODO: clone only if necessary
    match state.store.deref(func_ref).content.clone() {
        ObjectContent::Class(None) => {
            let frame = call_stack.last_mut().unwrap();
            frame.var_stack.push(state.store.allocate(Object::new_instance(None, func_ref.clone(), ObjectContent::OtherObject)))
        },
        ObjectContent::Class(Some(ref code_ref)) => {
            // TODO: run code
            let frame = call_stack.last_mut().unwrap();
            frame.var_stack.push(state.store.allocate(Object::new_instance(None, func_ref.clone(), ObjectContent::OtherObject)))
        },
        ObjectContent::Function(ref func_module, ref code_ref) => {
            let code = state.store.deref(code_ref).content.clone();
            if let ObjectContent::Code(code) = code {
                if code.co_varargs() { // If it has a *args argument
                    if code.argcount > args.len() {
                        panic!(format!("{}() takes at least {} arguments, but {} was/were given.", code.name, code.argcount, args.len()))
                    };
                    let to_vararg = args.drain(code.argcount..).collect();
                    let obj_ref = state.store.allocate(state.primitive_objects.new_tuple(to_vararg));
                    args.push(obj_ref);
                }
                else if code.argcount != args.len() {
                    panic!(format!("{}() takes {} arguments, but {} was/were given.", code.name, code.argcount, args.len()))
                };
                let mut locals = Rc::new(RefCell::new(HashMap::new()));
                {
                    let mut locals = locals.borrow_mut();
                    for (argname, argvalue) in code.varnames.iter().zip(args) {
                        locals.insert(argname.clone(), argvalue);
                    };
                }
                let new_frame = Frame::new(func_ref.clone(), *code, locals);
                call_stack.push(new_frame);
            }
            else {
                raise!(state, call_stack, state.primitive_objects.processorerror, format!("Not a code object {}", func_ref.repr(&state.store)));
            }
        },
        ObjectContent::PrimitiveFunction(ref name) => {
            let function_opt = state.primitive_functions.get(name).map(|o| *o);
            match function_opt {
                None => {
                    raise!(state, call_stack, state.primitive_objects.baseexception, format!("Unknown primitive {}", name)); // Should have errored before
                },
                Some(function) => {
                    let res = function(state, args); // Call the primitive
                    match res {
                        PyResult::Return(res) => call_stack.last_mut().unwrap().var_stack.push(res),
                        PyResult::Raised => ()
                    };
                }
            }
        },
        _ => {
            raise!(state, call_stack, state.primitive_objects.typeerror.clone(), format!("Not a function object {:?}", state.store.deref(func_ref)));
        }
    }
}

// Main interpreter loop
// See https://docs.python.org/3/library/dis.html for a description of instructions
fn run_code<EP: EnvProxy>(state: &mut State<EP>, call_stack: &mut Vec<Frame>) -> PyResult {
    loop {
        let instruction = {
            let frame = call_stack.last_mut().unwrap();
            let instruction = py_unwrap!(state, frame.instructions.get(frame.program_counter), ProcessorError::InvalidProgramCounter);
            frame.program_counter += 1;
            instruction.clone()
        };
        match instruction {
            Instruction::PopTop => {
                let frame = call_stack.last_mut().unwrap();
                pop_stack!(state, frame.var_stack);
                ()
            },
            Instruction::DupTop => {
                let frame = call_stack.last_mut().unwrap();
                let val = pop_stack!(state, frame.var_stack);
                frame.var_stack.push(val.clone());
                frame.var_stack.push(val);
            }
            Instruction::Nop => (),
            Instruction::BinarySubscr => {
                let (container, index) = {
                    let frame = call_stack.last_mut().unwrap();
                    let index_ref = pop_stack!(state, frame.var_stack);
                    let index = state.store.deref(&index_ref).content.clone();
                    let container_ref = pop_stack!(state, frame.var_stack);
                    let container = state.store.deref(&container_ref).content.clone();
                    (container, index)
                };
                match (container, index) {
                    (ObjectContent::Tuple(v), ObjectContent::Int(i)) | (ObjectContent::List(v), ObjectContent::Int(i)) => {
                        match v.get(i as usize) { // TODO: overflow check
                            None => raise!(state, call_stack, state.primitive_objects.nameerror, format!("Index out of range")),
                            Some(obj_ref) => {
                                let frame = call_stack.last_mut().unwrap();
                                frame.var_stack.push(obj_ref.clone())
                            },
                        }
                    },
                    (ObjectContent::Tuple(_), index) =>
                        raise!(state, call_stack, state.primitive_objects.typeerror.clone(), format!("tuple indices must be int, not {:?}", index)),
                    (ObjectContent::List(_), index) =>
                        raise!(state, call_stack, state.primitive_objects.typeerror.clone(), format!("list indices must be int, not {:?}", index)),
                    (container, _index) =>
                        raise!(state, call_stack, state.primitive_objects.typeerror.clone(), format!("{:?} object is not subscriptable", container)),
                }
            }
            Instruction::LoadBuildClass => {
                let frame = call_stack.last_mut().unwrap();
                let obj = Object { name: Some("__build_class__".to_string()), content: ObjectContent::PrimitiveFunction("build_class".to_string()), class: state.primitive_objects.function_type.clone(), bases: None };
                frame.var_stack.push(state.store.allocate(obj));
            }
            Instruction::ReturnValue => {
                let mut frame = call_stack.pop().unwrap();
                let result = pop_stack!(state, frame.var_stack);
                match call_stack.last_mut() {
                    Some(parent_frame) => parent_frame.var_stack.push(result),
                    None => return PyResult::Return(result), // End of program
                }
            }
            Instruction::PopBlock => {
                let frame = call_stack.last_mut().unwrap();
                pop_stack!(state, frame.block_stack);
            }
            Instruction::EndFinally => {
                let status_ref = {
                    let frame = call_stack.last_mut().unwrap();
                    pop_stack!(state, frame.var_stack)
                };
                let status = state.store.deref(&status_ref);
                match status.content {
                    ObjectContent::Int(i) => panic!("TODO: finally int status"), // TODO
                    ObjectContent::OtherObject => {
                        let (val, traceback) = {
                            let frame = call_stack.last_mut().unwrap();
                            let val = pop_stack!(state, frame.var_stack); // Note: CPython calls this variable “exc”
                            let traceback = pop_stack!(state, frame.var_stack);
                            (val, traceback)
                        };
                        let exc = status_ref;
                        call_stack.last_mut().unwrap().block_stack.pop().unwrap(); // Remove this try…except block
                        unwind(call_stack, traceback, exc, val);
                    }
                    ObjectContent::None => {
                    }
                    _ => panic!(format!("Invalid finally status: {:?}", status))
                }
            }
            Instruction::PopExcept => {
                let frame = call_stack.last_mut().unwrap();
                let mut three_last = frame.var_stack.pop_all_and_get_n_last(3).unwrap(); // TODO: check
                let exc_type = three_last.pop();
                let exc_value = three_last.pop();
                let exc_traceback = three_last.pop();
                // TODO: do something with exc_*
                pop_stack!(state, frame.block_stack);
            },
            Instruction::StoreName(i) => {
                let frame = call_stack.last_mut().unwrap();
                let name = py_unwrap!(state, frame.code.names.get(i), ProcessorError::InvalidNameIndex).clone();
                let obj_ref = pop_stack!(state, frame.var_stack);
                frame.locals.borrow_mut().insert(name, obj_ref);
            }
            Instruction::LoadConst(i) => {
                let frame = call_stack.last_mut().unwrap();
                frame.var_stack.push(py_unwrap!(state, frame.code.consts.get(i), ProcessorError::InvalidConstIndex).clone())
            }
            Instruction::LoadName(i) | Instruction::LoadGlobal(i) => { // TODO: LoadGlobal should look only in globals
                let (name, res) = {
                    let frame = call_stack.last_mut().unwrap();
                    let name = py_unwrap!(state, frame.code.names.get(i), ProcessorError::InvalidNameIndex);
                    let res = load_name(state, &frame, name);
                    (name.clone(), res)
                };
                match res {
                    None => raise!(state, call_stack, state.primitive_objects.nameerror, format!("Unknown variable {}", name)),
                    Some(obj_ref) => {
                        let frame = call_stack.last_mut().unwrap();
                        frame.var_stack.push(obj_ref)
                    }
                }
            }
            Instruction::LoadAttr(i) => {
                let (name, obj) = {
                    let frame = call_stack.last_mut().unwrap();
                    let name = py_unwrap!(state, frame.code.names.get(i), ProcessorError::InvalidNameIndex).clone();
                    let obj_ref = py_unwrap!(state, frame.var_stack.pop(), ProcessorError::StackTooSmall);
                    let obj = state.store.deref(&obj_ref).clone();
                    (name, obj)
                };
                let res = load_attr(state, &obj, &name);
                match res {
                    None => raise!(state, call_stack, state.primitive_objects.nameerror, format!("Unknown variable {}", name)),
                    Some(obj_ref) => {
                        let frame = call_stack.last_mut().unwrap();
                        frame.var_stack.push(obj_ref)
                    }
                }
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
                let obj1 = state.store.deref(&pop_stack!(state, frame.var_stack));
                let obj2 = state.store.deref(&pop_stack!(state, frame.var_stack));
                if obj1.name == obj2.name && obj1.content == obj2.content {
                    frame.var_stack.push(state.primitive_objects.true_obj.clone())
                }
                else {
                    frame.var_stack.push(state.primitive_objects.false_obj.clone())
                };
            }
            Instruction::CompareOp(CmpOperator::ExceptionMatch) => {
                let frame = call_stack.last_mut().unwrap();
                // TODO: add support for tuples
                let pattern_ref = pop_stack!(state, frame.var_stack);
                let exc_ref = pop_stack!(state, frame.var_stack);
                let isinstance = state.primitive_functions.get("isinstance").unwrap().clone();
                match isinstance(state, vec![exc_ref, pattern_ref]) {
                    PyResult::Return(val) => frame.var_stack.push(val),
                    PyResult::Raised => (),
                }
            }
            Instruction::JumpForward(delta) => {
                let frame = call_stack.last_mut().unwrap();
                frame.program_counter += delta
            }
            Instruction::LoadFast(i) => {
                let frame = call_stack.last_mut().unwrap();
                let name = py_unwrap!(state, frame.code.varnames.get(i), ProcessorError::InvalidVarnameIndex).clone();
                let obj_ref = py_unwrap!(state, frame.locals.borrow().get(&name), ProcessorError::InvalidName(name)).clone();
                frame.var_stack.push(obj_ref)
            }
            Instruction::PopJumpIfFalse(target) => {
                let frame = call_stack.last_mut().unwrap();
                let obj = state.store.deref(&pop_stack!(state, frame.var_stack));
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
                let exception = pop_stack!(state, call_stack.last_mut().unwrap().var_stack);
                unwind(call_stack, state.primitive_objects.none.clone(), exception, state.primitive_objects.none.clone());
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
                    kwargs = py_unwrap!(state, frame.var_stack.pop_many(nb_kwargs*2), ProcessorError::StackTooSmall);
                    args = py_unwrap!(state, frame.var_stack.pop_many(nb_args), ProcessorError::StackTooSmall);
                    func = pop_stack!(state, frame.var_stack);
                }
                call_function(state, call_stack, &func, args, kwargs)
            },
            Instruction::MakeFunction(0, 0, 0) => {
                // TODO: consume default arguments and annotations
                let obj = {
                    let frame = call_stack.last_mut().unwrap();
                    let obj = state.store.deref(&pop_stack!(state, frame.var_stack)).content.clone(); // TODO: clone only if necessary
                    obj
                };
                let func_name = match obj {
                    ObjectContent::String(ref s) => s.clone(),
                    name => {
                        raise!(state, call_stack, state.primitive_objects.typeerror.clone(), format!("function names must be strings, not {:?}", name));
                        continue
                    }
                };
                let frame = call_stack.last_mut().unwrap();
                let code = pop_stack!(state, frame.var_stack);
                let func = state.primitive_objects.new_function(func_name, frame.object.module(&state.store), code);
                frame.var_stack.push(state.store.allocate(func))
            },
            _ => panic!(format!("todo: instruction {:?}", instruction)),
        }
    };
}

fn call_module_code<EP: EnvProxy>(state: &mut State<EP>, call_stack: &mut Vec<Frame>, module_name: String, module_ref: ObjectRef) -> PyResult {
    let code_ref = match state.store.deref(&module_ref).content {
        ObjectContent::Module(ref code_ref) => code_ref.clone(),
        ref o => panic!("Not a module: {:?}", o),
    };
    let code = match state.store.deref(&code_ref).content {
        ObjectContent::Code(ref code) => code.clone(),
        ref o => return state.raise_processor_error(ProcessorError::NotACodeObject(format!("file code {:?}", o))),
    };
    let module_obj = state.store.allocate(state.primitive_objects.new_module(module_name.clone(), code_ref));
    state.modules.insert(module_name.clone(), Rc::new(RefCell::new(HashMap::new())));
    let mut call_stack = vec![Frame::new(module_obj, *code, state.modules.get(&module_name).unwrap().clone())];
    let res = run_code(state, &mut call_stack);
    res // Do not raise exceptions before the pop()
}

/// Get the code of a module from its name
pub fn get_module_code<EP: EnvProxy>(state: &mut State<EP>, call_stack: &mut Vec<Frame>, module_name: String) -> PyResult {
    // Load the code
    let mut module_bytecode = state.envproxy.open_module(module_name.clone());
    let mut buf = [0; 12];
    module_bytecode.read_exact(&mut buf).unwrap();
    if !marshal::check_magic(&buf[0..4]) {
        panic!(format!("Bad magic number for module {}.", module_name))
    }
    match marshal::read_object(&mut module_bytecode, &mut state.store, &state.primitive_objects) {
        Err(e) => state.raise_processor_error(ProcessorError::UnmarshalError(e)),
        Ok(module_code_ref) => {
            let module_code = match state.store.deref(&module_code_ref).content {
                ObjectContent::Code(ref code) => code.clone(),
                ref o => return state.raise_processor_error(ProcessorError::NotACodeObject(format!("module code {:?}", o))),
            };
            PyResult::Return(state.store.allocate(state.primitive_objects.new_module(module_name.clone(), module_code_ref)))
        }
    }
}

/// Entry point to run code. Loads builtins in the code's namespace and then run it.
pub fn call_main_code<EP: EnvProxy>(state: &mut State<EP>, code_ref: ObjectRef) -> PyResult {
    let mut call_stack = Vec::new();
    let builtins_code_ref = py_try!(get_module_code(state, &mut call_stack, "builtins".to_string()));
    py_try!(call_module_code(state, &mut call_stack, "builtins".to_string(), builtins_code_ref));

    let mut call_stack = Vec::new();
    let module_ref = state.store.allocate(state.primitive_objects.new_module("__main__".to_string(), code_ref));
    call_module_code(state, &mut call_stack, "__main__".to_string(), module_ref)
}
