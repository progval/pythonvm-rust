pub mod instructions;

use super::objects::{Code, ObjectStore, ObjectRef, ObjectContent};
use super::sandbox::EnvProxy;
use super::stack::{Stack, VectorStack};
use self::instructions::Instruction;

pub enum ProcessorError {
    CircularReference,
    InvalidReference,
    NotACodeObject,
    CodeObjectIsNotBytes,
    InvalidProgramCounter,
    StackTooSmall,
    InvalidConstIndex,
    InvalidNameIndex,
}

fn call_function<EP: EnvProxy>(envproxy: &mut EP, store: &mut ObjectStore, func_ref: &ObjectRef, args: Vec<ObjectRef>, kwags: Vec<ObjectRef>) -> Result<ObjectRef, ProcessorError> {
    let code = match store.deref(func_ref).content {
        ObjectContent::Code(ref code) => code.clone(),
        _ => return Err(ProcessorError::NotACodeObject),
    };
    run_code(envproxy, store, code)
}

fn run_code<EP: EnvProxy>(envproxy: &mut EP, store: &mut ObjectStore, code: Code) -> Result<ObjectRef, ProcessorError> {
    let bytecode: Vec<u8> = code.code;
    let instructions: Vec<Instruction> = instructions::InstructionDecoder::new(bytecode.iter()).into_iter().collect();
    let mut program_counter = 0 as usize;
    let mut stack = VectorStack::new();
    loop {
        let instruction = try!(instructions.get(program_counter).ok_or(ProcessorError::InvalidProgramCounter));
        program_counter += 1;
        match *instruction {
            Instruction::ReturnValue => return Ok(try!(stack.pop().ok_or(ProcessorError::StackTooSmall))),
            Instruction::LoadConst(i) => stack.push(try!(code.names.get(i).ok_or(ProcessorError::InvalidConstIndex)).clone()),
            Instruction::LoadName(i) => stack.push(try!(code.names.get(i).ok_or(ProcessorError::InvalidNameIndex)).clone()),
            Instruction::CallFunction(nb_args, nb_kwargs) => {
                // See “Call constructs” at:
                // http://security.coverity.com/blog/2014/Nov/understanding-python-bytecode.html
                let kwargs = try!(stack.pop_many(nb_kwargs*2).ok_or(ProcessorError::StackTooSmall));
                let args = try!(stack.pop_many(nb_args).ok_or(ProcessorError::StackTooSmall));
                let func = try!(stack.pop().ok_or(ProcessorError::StackTooSmall));
                let ret_value = call_function(envproxy, store, &func, args, kwargs);
                stack.push(try!(ret_value))
            }
            _ => panic!(format!("todo: instruction {:?}", *instruction)),
        }
    };
    panic!("Unreachable")
}

pub fn run_code_object<EP: EnvProxy>(envproxy: &mut EP, store: &mut ObjectStore, module: ObjectRef) -> Result<ObjectRef, ProcessorError> {
    let code = match store.deref(&module).content {
        ObjectContent::Code(ref code) => code.clone(),
        _ => return Err(ProcessorError::NotACodeObject),
    };
    run_code(envproxy, store, code)
}
