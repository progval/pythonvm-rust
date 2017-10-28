pub mod instructions;
pub mod frame;

use super::objects::{ObjectRef, ObjectContent, Object};
use super::varstack::VarStack;
use self::instructions::{CmpOperator, Instruction};
use self::frame::{Block, Frame};
use std::fmt;
use std::collections::HashMap;
use std::io::Read;
use std::cell::RefCell;
use std::rc::Rc;
use super::marshal;
use super::state::{State, PyResult, unwind, raise, return_value};
use super::sandbox::EnvProxy;
use super::primitives;

const WORD_SIZE: usize = 2;

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

macro_rules! top_stack {
    ( $state: expr, $stack_name: expr) => {
        py_unwrap!($state, $stack_name.top(), ProcessorError::StackTooSmall)
    }
}



// Load a name from the namespace
fn load_name<EP: EnvProxy>(state: &mut State<EP>, frame: &Frame, name: &String) -> Option<ObjectRef> {
    if *name == "__primitives__" {
        return Some(state.store.allocate(Object::new_instance(Some("__primitives__".to_string()), state.primitive_objects.object.clone(), ObjectContent::PrimitiveNamespace)))
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
                    None => Some(state.store.allocate(Object::new_instance(Some(name.clone()), state.primitive_objects.function_type.clone(), ObjectContent::PrimitiveFunction(name.clone())))),
                }
            }
            else {
                // TODO: special names
                match obj.attributes {
                    Some(ref attributes) => attributes.borrow().get(name).map(|r| r.clone()),
                    None => None,
                }
            }
        }
    }
}

// Call a primitive / function / code object, with arguments.
fn call_function<EP: EnvProxy>(state: &mut State<EP>, call_stack: &mut Vec<Frame>, func_ref: &ObjectRef, mut args: Vec<ObjectRef>, kwargs: Vec<(ObjectRef, ObjectRef)>) {
    // TODO: clone only if necessary
    match state.store.deref(func_ref).content.clone() {
        ObjectContent::Class => {
            let frame = call_stack.last_mut().unwrap();
            frame.var_stack.push(func_ref.new_instance(&mut state.store, args, kwargs))
        },
        ObjectContent::Function(ref _func_module, ref code_ref, ref defaults) => {
            let code = state.store.deref(code_ref).content.clone();
            if let ObjectContent::Code(code) = code {

                let mut locals = defaults.clone();

                if let Some(starargs_name) = code.get_varargs_name() { // If it has a *args argument
                    if code.argcount > args.len() {
                        panic!(format!("{}() takes at least {} arguments, but {} was/were given.", code.name, code.argcount, args.len()))
                    };
                    let to_vararg = args.drain(code.argcount..).collect();
                    let obj_ref = state.store.allocate(state.primitive_objects.new_tuple(to_vararg));

                    // Bind *args
                    assert_eq!(None, locals.insert(starargs_name.clone(), obj_ref));
                }
                else if code.argcount != args.len() { // If it has no *args argument
                    panic!(format!("{}() takes {} arguments, but {} was/were given.", code.name, code.argcount, args.len()))
                };

                // Handle keyword arguments
                let mut remaining_kwargs = vec![]; // arguments that will go to **kwargs
                {
                    let explicit_keywords = code.keywords();
                    for (key, value) in kwargs.into_iter() {
                        let key_str = match state.store.deref(&key).content {
                            ObjectContent::String(ref s) => s,
                            _ => panic!("Keyword names should be strings."),
                        };
                        if explicit_keywords.contains(key_str) {
                            locals.insert(key_str.clone(), value);
                        }
                        else {
                            remaining_kwargs.push((key, value))
                        }
                    }
                }

                if let Some(starkwargs_name) = code.get_varkwargs_name() { // If it has a **kwargs argument
                    let obj_ref = state.store.allocate(state.primitive_objects.new_dict(remaining_kwargs));
                    locals.insert(starkwargs_name.clone(), obj_ref);
                }
                else { // If it has no **kwargs argument
                    if remaining_kwargs.len() != 0 {
                        panic!(format!("Unknown keyword argument to function {} with no **kwargs", code.name))
                    }
                }

                // Bind positional arguments
                {
                    for (argname, argvalue) in code.varnames.iter().zip(args) {
                        locals.insert(argname.clone(), argvalue);
                    };
                }

                let new_frame = Frame::new(func_ref.clone(), *code, Rc::new(RefCell::new(locals)));
                call_stack.push(new_frame);
            }
            else {
                let exc = state.primitive_objects.processorerror.clone();
                let repr = code_ref.repr(&state.store);
                raise(state, call_stack, exc, format!("Not a code object {}", repr));
            }
        },
        ObjectContent::PrimitiveFunction(ref name) => {
            let function_opt = state.primitive_functions.get(name).map(|o| *o);
            match function_opt {
                None => {
                    let exc = state.primitive_objects.baseexception.clone();
                    raise(state, call_stack, exc, format!("Unknown primitive {}", name)); // Should have errored before
                },
                Some(function) => {
                    function(state, call_stack, args); // Call the primitive
                }
            }
        },
        _ => {
            let exc = state.primitive_objects.typeerror.clone();
            let repr = func_ref.repr(&state.store);
            raise(state, call_stack, exc, format!("Not a function object {:?}", repr));
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
            // Useful for debugging:
            /*
            println!("======");
            for r in frame.var_stack.iter() {
                println!("{}", r.repr(&state.store));
            }
            println!("{} {:?}", frame.program_counter, instruction);
            println!("======");
            */
            frame.program_counter += 1;
            instruction.clone()
        };
        match instruction {
            Instruction::PushImmediate(r) => {
                let frame = call_stack.last_mut().unwrap();
                frame.var_stack.push(r);
            },
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
                let typeerror = state.primitive_objects.typeerror.clone();
                match (container, index) {
                    (ObjectContent::Tuple(v), ObjectContent::Int(i)) | (ObjectContent::List(v), ObjectContent::Int(i)) => {
                        match v.get(i as usize) { // TODO: overflow check
                            None => {
                                let exc = state.primitive_objects.nameerror.clone();
                                raise(state, call_stack, exc, format!("Index out of range"))
                            },
                            Some(obj_ref) => {
                                let frame = call_stack.last_mut().unwrap();
                                frame.var_stack.push(obj_ref.clone())
                            },
                        }
                    },
                    (ObjectContent::Tuple(_), index) =>
                        raise(state, call_stack, typeerror, format!("tuple indices must be int, not {:?}", index)),
                    (ObjectContent::List(_), index) =>
                        raise(state, call_stack, typeerror, format!("list indices must be int, not {:?}", index)),
                    (container, _index) =>
                        raise(state, call_stack, typeerror, format!("{:?} object is not subscriptable", container)),
                }
            }
            Instruction::GetIter => {
                let frame = call_stack.last_mut().unwrap();
                let obj_ref = pop_stack!(state, frame.var_stack);
                frame.var_stack.push(obj_ref.iter(state));
            }
            Instruction::LoadBuildClass => {
                let frame = call_stack.last_mut().unwrap();
                let obj = Object::new_instance(Some("__build_class__".to_string()), state.primitive_objects.function_type.clone(), ObjectContent::PrimitiveFunction("build_class".to_string()));
                frame.var_stack.push(state.store.allocate(obj));
            }
            Instruction::ReturnValue => {
                if call_stack.len() == 1 {
                    let mut frame = call_stack.pop().unwrap();
                    let result = pop_stack!(state, frame.var_stack);
                    return PyResult::Return(result);
                }
                else {
                    let mut frame = call_stack.pop().unwrap();
                    let result = pop_stack!(state, frame.var_stack);
                    return_value(call_stack, result)
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
                let status_content = {
                    let status = state.store.deref(&status_ref);
                    let content = status.content.clone(); // TODO: copy only if needed
                    content
                };
                match status_content {
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
                        unwind(state, call_stack, traceback, exc, val);
                    }
                    ObjectContent::None => {
                    }
                    _ => panic!(format!("Invalid finally status: {:?}", state.store.deref(&status_ref)))
                }
            }
            Instruction::PopExcept => {
                let frame = call_stack.last_mut().unwrap();
                let mut three_last = frame.var_stack.pop_all_and_get_n_last(3).unwrap(); // TODO: check
                let _exc_type = three_last.pop();
                let _exc_value = three_last.pop();
                let _exc_traceback = three_last.pop();
                // TODO: do something with exc_*
                pop_stack!(state, frame.block_stack);
            },
            Instruction::StoreName(i) => {
                let frame = call_stack.last_mut().unwrap();
                let name = py_unwrap!(state, frame.code.names.get(i), ProcessorError::InvalidNameIndex).clone();
                let obj_ref = pop_stack!(state, frame.var_stack);
                frame.locals.borrow_mut().insert(name, obj_ref);
            }
            Instruction::ForIter(i) => {
                let iterator = {
                    let frame = call_stack.last_mut().unwrap();
                    frame.block_stack.push(Block::ExceptPopGoto(state.primitive_objects.stopiteration.clone(), 1, frame.program_counter+i/WORD_SIZE));
                    let iterator = top_stack!(state, frame.var_stack);
                    iterator.clone()
                };
                let iter_func = state.store.allocate(Object::new_instance(None, state.primitive_objects.function_type.clone(), ObjectContent::PrimitiveFunction("iter".to_string())));
                call_function(state, call_stack, &iter_func, vec![iterator], vec![]);
            }
            Instruction::StoreAttr(i) => {
                let frame = call_stack.last_mut().unwrap();
                let name = py_unwrap!(state, frame.code.names.get(i), ProcessorError::InvalidNameIndex).clone();
                let owner = pop_stack!(state, frame.var_stack);
                let value = pop_stack!(state, frame.var_stack);
                owner.setattr(&mut state.store, name, value);
            }
            Instruction::StoreGlobal(i) => {
                let frame = call_stack.last_mut().unwrap();
                let name = py_unwrap!(state, frame.code.varnames.get(i), ProcessorError::InvalidVarnameIndex).clone();
                let mut globals = state.modules.get(&frame.object.module(&state.store)).unwrap().borrow_mut();
                globals.insert(name, pop_stack!(state, frame.var_stack));
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
                    None => {
                        let exc = state.primitive_objects.nameerror.clone();
                        raise(state, call_stack, exc, format!("Unknown variable {}", name))
                    },
                    Some(obj_ref) => {
                        let frame = call_stack.last_mut().unwrap();
                        frame.var_stack.push(obj_ref)
                    }
                }
            }
            Instruction::BuildTuple(size) => {
                let frame = call_stack.last_mut().unwrap();
                let content = py_unwrap!(state, frame.var_stack.pop_many(size), ProcessorError::StackTooSmall);
                let tuple = state.primitive_objects.new_tuple(content);
                frame.var_stack.push(state.store.allocate(tuple));
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
                    None => {
                        let exc = state.primitive_objects.nameerror.clone();
                        raise(state, call_stack, exc, format!("Unknown attribute {}", name))
                    },
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
                frame.block_stack.push(Block::TryExcept(frame.program_counter, frame.program_counter+i/WORD_SIZE))
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
                let val = if primitives::native_isinstance(&state.store, &exc_ref, &pattern_ref) {
                    state.primitive_objects.true_obj.clone()
                }
                else {
                    state.primitive_objects.false_obj.clone()
                };
                frame.var_stack.push(val)
            }
            Instruction::JumpAbsolute(target) => {
                let frame = call_stack.last_mut().unwrap();
                frame.program_counter = target / WORD_SIZE
            }
            Instruction::JumpForward(delta) => {
                let frame = call_stack.last_mut().unwrap();
                frame.program_counter += delta / WORD_SIZE
            }
            Instruction::LoadFast(i) => {
                let frame = call_stack.last_mut().unwrap();
                let name = py_unwrap!(state, frame.code.varnames.get(i), ProcessorError::InvalidVarnameIndex).clone();
                let obj_ref = py_unwrap!(state, frame.locals.borrow().get(&name), ProcessorError::InvalidName(name)).clone();
                frame.var_stack.push(obj_ref)
            }
            Instruction::StoreFast(i) => {
                let frame = call_stack.last_mut().unwrap();
                let name = py_unwrap!(state, frame.code.varnames.get(i), ProcessorError::InvalidVarnameIndex).clone();
                frame.locals.borrow_mut().insert(name, pop_stack!(state, frame.var_stack));
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
                let traceback = state.primitive_objects.none.clone();
                let value = state.primitive_objects.none.clone();
                unwind(state, call_stack, traceback, exception, value);
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
                let kwargs;
                let args;
                let func;
                {
                    let frame = call_stack.last_mut().unwrap();
                    kwargs = py_unwrap!(state, frame.var_stack.pop_n_pairs(nb_kwargs), ProcessorError::StackTooSmall);
                    args = py_unwrap!(state, frame.var_stack.pop_many(nb_args), ProcessorError::StackTooSmall);
                    func = pop_stack!(state, frame.var_stack);
                }
                call_function(state, call_stack, &func, args, kwargs)
            },
            Instruction::MakeFunction { has_defaults: false, has_kwdefaults, has_annotations: false, has_closure: false } => {
                // TODO: consume default arguments and annotations
                let obj = {
                    let frame = call_stack.last_mut().unwrap();
                    let obj = state.store.deref(&pop_stack!(state, frame.var_stack)).content.clone(); // TODO: clone only if necessary
                    obj
                };
                let func_name = match obj {
                    ObjectContent::String(ref s) => s.clone(),
                    name => {
                        let exc = state.primitive_objects.typeerror.clone();
                        raise(state, call_stack, exc, format!("function names must be strings, not {:?}", name));
                        continue
                    }
                };
                let frame = call_stack.last_mut().unwrap();
                let code = pop_stack!(state, frame.var_stack);
                let mut kwdefaults: HashMap<String, ObjectRef> = HashMap::new();
                if has_kwdefaults {
                    let obj = state.store.deref(&pop_stack!(state, frame.var_stack)).content.clone(); // TODO: clone only if necessary
                    let raw_kwdefaults = match obj {
                        ObjectContent::Dict(ref d) => d,
                        _ => panic!("bad type for default kwd"),
                    };
                    kwdefaults.reserve(raw_kwdefaults.len());
                    for &(ref key, ref value) in raw_kwdefaults {
                        match state.store.deref(&key).content {
                            ObjectContent::String(ref s) => { kwdefaults.insert(s.clone(), value.clone()); },
                            _ => panic!("Defaults' keys must be strings."),
                        }
                    }
                }
                let func = state.primitive_objects.new_function(func_name, frame.object.module(&state.store), code, kwdefaults);
                frame.var_stack.push(state.store.allocate(func))
            },
            Instruction::BuildConstKeyMap(size) => {
                let frame = call_stack.last_mut().unwrap();
                let obj = state.store.deref(&pop_stack!(state, frame.var_stack)).content.clone(); // TODO: clone only if necessary
                let keys: Vec<ObjectRef> = match obj {
                    ObjectContent::Tuple(ref v) => v.clone(),
                    _ => panic!("bad BuildConstKeyMap keys argument."),
                };
                let values: Vec<ObjectRef> = frame.var_stack.peek(size).unwrap().iter().map(|r| (*r).clone()).collect();
                let dict = state.primitive_objects.new_dict(keys.into_iter().zip(values).collect());
                frame.var_stack.push(state.store.allocate(dict))
            }
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
    call_stack.push(Frame::new(module_obj, *code, state.modules.get(&module_name).unwrap().clone()));
    let res = run_code(state, call_stack);
    res // Do not raise exceptions before the pop()
}

/// Get the code of a module from its name
pub fn get_module_code<EP: EnvProxy>(state: &mut State<EP>, module_name: String) -> PyResult {
    // Load the code
    let mut module_bytecode = state.envproxy.open_module(module_name.clone());
    let mut buf = [0; 12];
    module_bytecode.read_exact(&mut buf).unwrap();
    if !marshal::check_magic(&buf[0..4]) {
        panic!(format!("Bad magic number for module {}.", module_name))
    }
    match marshal::read_object(&mut module_bytecode, &mut state.store, &state.primitive_objects) {
        Err(e) => state.raise_processor_error(ProcessorError::UnmarshalError(e)),
        Ok(module_code_ref) => PyResult::Return(state.store.allocate(state.primitive_objects.new_module(module_name.clone(), module_code_ref))),
    }
}

/// Entry point to run code. Loads builtins in the code's namespace and then run it.
pub fn call_main_code<EP: EnvProxy>(state: &mut State<EP>, code_ref: ObjectRef) -> PyResult {
    let mut call_stack = Vec::new();
    let builtins_code_ref = py_try!(get_module_code(state, "builtins".to_string()));
    py_try!(call_module_code(state, &mut call_stack, "builtins".to_string(), builtins_code_ref));

    let mut call_stack = Vec::new();
    let module_ref = state.store.allocate(state.primitive_objects.new_module("__main__".to_string(), code_ref));
    call_module_code(state, &mut call_stack, "__main__".to_string(), module_ref)
}
