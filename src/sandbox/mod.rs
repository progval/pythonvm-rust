use std::io;
use std::sync::{Arc, Mutex};

/// Real environment (I/O, signals, …) or a mock
pub trait EnvProxy {
    type Stdout: io::Write;
    fn stdout(&self) -> Self::Stdout;
}



/// An EnvProxy that exposes the real environment
pub struct RealEnvProxy {
}

impl RealEnvProxy {
    pub fn new() -> RealEnvProxy {
        RealEnvProxy { }
    }
}

impl EnvProxy for RealEnvProxy {
    type Stdout = io::Stdout;
    fn stdout(&self) -> Self::Stdout {
        io::stdout()
    }
}



struct VectorWriter {
    vector: Arc<Mutex<Vec<u8>>>,
}

impl VectorWriter {
    pub fn new(vector: Arc<Mutex<Vec<u8>>>) -> VectorWriter {
        VectorWriter { vector: vector }
    }
}

impl io::Write for VectorWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.vector.lock().unwrap().extend(buf); // TODO: remove unwrap()
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub struct MockEnvProxy {
    stdout_content: Arc<Mutex<Vec<u8>>>,
}

impl MockEnvProxy {
    pub fn new() -> MockEnvProxy {
        MockEnvProxy { stdout_content: Arc::new(Mutex::new(vec![])) }
    }
}


impl EnvProxy for MockEnvProxy {
    type Stdout = VectorWriter;
    fn stdout(&self) -> Self::Stdout {
        VectorWriter::new(self.stdout_content.clone())
    }
}
