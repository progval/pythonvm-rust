use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use super::sandbox::EnvProxy;
use super::objects::{ObjectStore, ObjectRef, PrimitiveObjects};
use super::processor::ProcessorError;

#[derive(Debug)]
#[must_use]
pub enum PyResult {
    Return(ObjectRef),
    Raised, // Should only be returned after unwinding the call stack
}

pub type PyFunction<EP> = fn(&mut State<EP>, Vec<ObjectRef>) -> PyResult;

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
