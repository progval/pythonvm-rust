use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use super::super::varstack::{VarStack, VectorVarStack};
use super::super::objects::{ObjectRef, ObjectStore, ObjectContent, Code};
use super::instructions::{Instruction, InstructionDecoder};

#[derive(Debug)]
pub enum Block {
    Loop(usize, usize), // begin, end
    TryExcept(usize, usize), // begin, end
}

#[derive(Debug)]
pub struct Frame {
    pub object: ObjectRef,
    pub var_stack: VectorVarStack<ObjectRef>,
    pub block_stack: Vec<Block>,
    pub locals: Rc<RefCell<HashMap<String, ObjectRef>>>,
    pub instructions: Vec<Instruction>,
    pub code: Code,
    pub program_counter: usize,
}

impl Frame {
    pub fn new(object: ObjectRef, code: Code, locals: Rc<RefCell<HashMap<String, ObjectRef>>>) -> Frame {
        let instructions: Vec<Instruction> = InstructionDecoder::new(code.code.iter()).into_iter().collect();
        Frame {
            object: object,
            var_stack: VectorVarStack::new(),
            block_stack: Vec::new(),
            locals: locals,
            instructions: instructions,
            code: code,
            program_counter: 0,
        }
    }
}

