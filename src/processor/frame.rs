use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use super::super::varstack::{VarStack, VectorVarStack};
use super::super::objects::{ObjectRef, ObjectStore, ObjectContent};

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
}

impl Frame {
    pub fn new(object: ObjectRef, locals: Rc<RefCell<HashMap<String, ObjectRef>>>) -> Frame {
        Frame {
            object: object,
            var_stack: VectorVarStack::new(),
            block_stack: Vec::new(),
            locals: locals,
        }
    }
}

