mod marshal;
mod objects;
mod processor;
mod sandbox;
mod stack;
mod primitives;

use std::fmt;
use std::io;
use processor::{PyResult, Processor};

pub use sandbox::{EnvProxy, RealEnvProxy, MockEnvProxy};

#[derive(Debug)]
pub enum InterpreterError {
    Io(io::Error),
    Unmarshal(marshal::decode::UnmarshalError),
    Processor(processor::ProcessorError),
}

impl fmt::Display for InterpreterError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            InterpreterError::Io(ref e) => write!(f, "I/O error: ").and_then(|_| e.fmt(f)),
            InterpreterError::Unmarshal(ref e) => write!(f, "Unmarshal error: ").and_then(|_| e.fmt(f)),
            InterpreterError::Processor(ref e) => write!(f, "Processor error: ").and_then(|_| e.fmt(f)),
        }
    }
}

pub fn run_file<R: io::Read, EP: sandbox::EnvProxy>(reader: &mut R, envproxy: EP) -> Result<(Processor<EP>, PyResult), InterpreterError> {
    let mut buf = [0; 12];
    try!(reader.read_exact(&mut buf).map_err(InterpreterError::Io));
    if !marshal::check_magic(&buf[0..4]) {
        panic!("Bad magic number for main file.")
    }
    let mut store = objects::ObjectStore::new();
    let primitive_objects = objects::PrimitiveObjects::new(&mut store);
    let module = try!(marshal::read_object(reader, &mut store, &primitive_objects).map_err(InterpreterError::Unmarshal));
    let mut processor = Processor { envproxy: envproxy, store: store, primitive_functions: primitives::get_default_primitives(), primitive_objects: primitive_objects };
    let result = processor.run_code_object(module);
    Ok((processor, result))
}

