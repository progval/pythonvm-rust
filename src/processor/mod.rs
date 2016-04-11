pub mod instructions;

use super::objects::{Object, Code, ObjectStore, ObjectRef, ObjectContent};
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

fn run_code<EP: EnvProxy>(envproxy: &mut EP, code: Code, store: &mut ObjectStore) -> Result<ObjectRef, ProcessorError> {
    let bytecode: Vec<u8> = code.code;
    let instructions: Vec<Instruction> = instructions::InstructionDecoder::new(bytecode.iter()).into_iter().collect();
    let mut program_counter = 0 as usize;
    let mut stack = VectorStack::new();
    while true {
        let instruction = try!(instructions.get(program_counter).ok_or(ProcessorError::InvalidProgramCounter));
        program_counter += 1;
        match *instruction {
            Instruction::ReturnValue => return Ok(try!(stack.pop().ok_or(ProcessorError::StackTooSmall))),
            Instruction::LoadConst(i) => stack.push(try!(code.names.get(i).ok_or(ProcessorError::InvalidConstIndex)).clone()),
            Instruction::LoadName(i) => stack.push(try!(code.names.get(i).ok_or(ProcessorError::InvalidNameIndex)).clone()),
            _ => panic!(format!("todo: instruction {:?}", *instruction)),
        }
    };
    panic!("Unreachable")
}

pub fn run_code_object<EP: EnvProxy>(envproxy: &mut EP, module: ObjectRef, store: &mut ObjectStore) -> Result<ObjectRef, ProcessorError> {
    let code = match store.deref(&module).content {
        ObjectContent::Code(ref code) => code.clone(),
        _ => return Err(ProcessorError::NotACodeObject),
    };
    run_code(envproxy, code, store)
}
