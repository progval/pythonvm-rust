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
    // TODO: do something with the content of the buffer
    let mut store = objects::ObjectStore::new();
    let module = try!(marshal::read_object(reader, &mut store).map_err(InterpreterError::Unmarshal));
    let mut processor = Processor { envproxy: envproxy, store: store, primitives: primitives::get_default_primitives() };
    let result = try!(processor.run_code_object(module).map_err(InterpreterError::Processor));
    Ok((processor, result))
}

