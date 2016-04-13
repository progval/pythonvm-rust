pub mod instructions;

use super::objects::{Code, ObjectStore, ObjectRef, ObjectContent};
use super::sandbox::EnvProxy;
use super::stack::{Stack, VectorStack};
use self::instructions::Instruction;
use std::fmt;
use std::collections::HashMap;
use std::io::Write;

#[derive(Debug)]
pub enum ProcessorError {
    CircularReference,
    InvalidReference,
    NotACodeObject(String),
    CodeObjectIsNotBytes,
    InvalidProgramCounter,
    StackTooSmall,
    InvalidConstIndex,
    InvalidNameIndex,
    UnknownBuiltin(String),
    Exception(String),
}

impl fmt::Display for ProcessorError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
       fmt::Debug::fmt(self, f)
    }
}

fn builtin_print<EP: EnvProxy>(processor: &mut Processor<EP>, args: Vec<ObjectRef>, kwargs: HashMap<String, ObjectRef>) -> Result<ObjectRef, ProcessorError> {
    if args.len() > 1 {
        return Err(ProcessorError::Exception(format!("print takes exactly one argument, not {}.", args.len())))
    }
    else {
        let obj_ref = args.get(0).unwrap();
        match processor.store.deref(obj_ref).content {
            ObjectContent::String(ref s) => {
                processor.envproxy.stdout().write(s.clone().into_bytes().as_slice()).unwrap(); // TODO: check
                processor.envproxy.stdout().write(b"\n").unwrap(); // TODO: check
            }
            ref o => return Err(ProcessorError::Exception(format!("print takes a string, not {:?}", o))),
        }
    }
    Ok(processor.store.allocate(ObjectContent::None))
}

pub type PyFunction<EP> = fn(&mut Processor<EP>, /*args:*/ Vec<ObjectRef>, /*kwargs:*/ HashMap<String, ObjectRef>) -> Result<ObjectRef, ProcessorError>;

pub struct Processor<EP: EnvProxy> {
    pub envproxy: EP,
    pub store: ObjectStore,
    pub builtin_functions: HashMap<String, PyFunction<EP>>,
}

impl<EP: EnvProxy> Processor<EP> {
    pub fn get_default_builtins() -> HashMap<String, PyFunction<EP>> {
        let mut builtins: HashMap<String, PyFunction<EP>> = HashMap::new();
        builtins.insert("print".to_string(), builtin_print);
        builtins
    }

    fn load_name(&mut self, name: String) -> Result<ObjectRef, ProcessorError> {
        Ok(self.store.allocate(ObjectContent::BuiltinFunction(name)))
    }

    fn call_function(&mut self, func_ref: &ObjectRef, args: Vec<ObjectRef>, kwargs: Vec<ObjectRef>) -> Result<ObjectRef, ProcessorError> {
        // TODO: clone only if necessary
        match self.store.deref(func_ref).content.clone() {
            ObjectContent::Code(code) => {
                self.run_code(code)
            },
            ObjectContent::BuiltinFunction(name) => {
                let f = match self.builtin_functions.get(&name) {
                    Some(f) => f.clone(),
                    None => return Err(ProcessorError::UnknownBuiltin(name.clone())),
                };
                f(self, args, HashMap::new()) // TODO: use the real kwargs
            },
            ref o => return Err(ProcessorError::NotACodeObject(format!("{:?}", o))),
        }
    }

    fn run_code(&mut self, code: Code) -> Result<ObjectRef, ProcessorError> {
        let bytecode: Vec<u8> = code.code;
        let instructions: Vec<Instruction> = instructions::InstructionDecoder::new(bytecode.iter()).into_iter().collect();
        let mut program_counter = 0 as usize;
        let mut stack = VectorStack::new();
        while true {
            let instruction = try!(instructions.get(program_counter).ok_or(ProcessorError::InvalidProgramCounter));
            program_counter += 1;
            match *instruction {
                Instruction::PopTop => {
                    try!(stack.pop().ok_or(ProcessorError::StackTooSmall));
                    ()
                },
                Instruction::ReturnValue => return Ok(try!(stack.pop().ok_or(ProcessorError::StackTooSmall))),
                Instruction::LoadConst(i) => stack.push(try!(code.consts.get(i).ok_or(ProcessorError::InvalidConstIndex)).clone()),
                Instruction::LoadName(i) => {
                    let name = try!(code.names.get(i).ok_or(ProcessorError::InvalidNameIndex)).clone();
                    stack.push(try!(self.load_name(name)))
                }
                Instruction::CallFunction(nb_args, nb_kwargs) => {
                    // See “Call constructs” at:
                    // http://security.coverity.com/blog/2014/Nov/understanding-python-bytecode.html
                    let kwargs = try!(stack.pop_many(nb_kwargs*2).ok_or(ProcessorError::StackTooSmall));
                    let args = try!(stack.pop_many(nb_args).ok_or(ProcessorError::StackTooSmall));
                    let func = try!(stack.pop().ok_or(ProcessorError::StackTooSmall));
                    let ret_value = self.call_function(&func, args, kwargs);
                    stack.push(try!(ret_value))
                }
                _ => panic!(format!("todo: instruction {:?}", *instruction)),
            }
        };
        panic!("Unreachable")
    }

    pub fn run_code_object(&mut self, module: ObjectRef) -> Result<ObjectRef, ProcessorError> {
        let code = match self.store.deref(&module).content {
            ObjectContent::Code(ref code) => code.clone(),
            ref o => return Err(ProcessorError::NotACodeObject(format!("{:?}", o))),
        };
        self.run_code(code)
    }
}
