use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use super::sandbox::EnvProxy;
use super::objects::{Code, ObjectStore, ObjectRef, ObjectContent, PrimitiveObjects, Object};
use super::processor::ProcessorError;
use super::processor::frame::{Block, Frame};
use super::primitives;
use super::varstack::VarStack;

#[derive(Debug)]
#[must_use]
pub enum PyResult {
    Return(ObjectRef),
    Raised, // Should only be returned after unwinding the call stack
}

pub type PyFunction<EP> = fn(&mut State<EP>, &mut Vec<Frame>, Vec<ObjectRef>);

pub struct State<EP: EnvProxy> {
    pub envproxy: EP,
    pub store: ObjectStore,
    pub primitive_functions: HashMap<String, PyFunction<EP>>,
    pub primitive_objects: PrimitiveObjects,
    pub modules: HashMap<String, Rc<RefCell<HashMap<String, ObjectRef>>>>,
}

impl<EP: EnvProxy> State<EP> {
    pub fn raise_processor_error(&self, error: ProcessorError) -> PyResult {
        // TODO: implement this
        panic!(format!("Runtime Error: {:?}", error))
    }
}

/// Unwind call stack until a try…except is found.
pub fn unwind<EP: EnvProxy>(state: &mut State<EP>, call_stack: &mut Vec<Frame>, traceback: ObjectRef, exception: ObjectRef, value: ObjectRef) {
    let exc_type = exception.clone(); // Looks like that's how CPython does things…
    'outer: loop {
        match call_stack.pop() {
            None => panic!("Exception {:?} reached bottom of call stack.", exception.repr(&state.store)),
            Some(mut frame) => {
                // Unwind block stack
                while let Some(block) = frame.block_stack.pop() {
                    match block {
                        Block::Loop(begin, end) => { // Non-try…except block, exit it.
                        }
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
                        Block::ExceptPopGoto(pattern, nb_pop, target) => {
                            if primitives::native_isinstance(&state.store, &exception, &pattern) {
                                frame.var_stack.pop_many(nb_pop);
                                frame.program_counter = target;
                                call_stack.push(frame);
                                break 'outer
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn raise<EP: EnvProxy>(state: &mut State<EP>, call_stack: &mut Vec<Frame>, exc_class: ObjectRef, msg: String) {
    let exc = Object::new_instance(None, exc_class, ObjectContent::String(msg));
    let exc = state.store.allocate(exc);
    // TODO: actual traceback
    let traceback = state.primitive_objects.none.clone();
    let value = state.primitive_objects.none.clone();
    unwind(state, call_stack, traceback, exc, value)
}

pub fn return_value(call_stack: &mut Vec<Frame>, result: ObjectRef) {
    match call_stack.last_mut() {
        Some(parent_frame) => parent_frame.var_stack.push(result),
        None => (), // End of program
    }
}
