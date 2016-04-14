use std::io;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::path::PathBuf;
use std::io::Bytes;
use std::io::Read;
use std::fs::File;

use super::objects::{ObjectRef, ObjectStore};

/// Real environment (I/O, signals, …) or a mock
pub trait EnvProxy {
    type Stdout: io::Write;
    fn stdout(&self) -> Self::Stdout;

    type ModuleBytecode: io::Read;
    fn open_module(&self, name: String) -> Self::ModuleBytecode;
}



/// An EnvProxy that exposes the real environment
pub struct RealEnvProxy {
    libdir: PathBuf,
}

impl RealEnvProxy {
    pub fn new(libdir: PathBuf) -> RealEnvProxy {
        RealEnvProxy { libdir: libdir }
    }
}

impl EnvProxy for RealEnvProxy {
    type Stdout = io::Stdout;
    fn stdout(&self) -> Self::Stdout {
        io::stdout()
    }

    type ModuleBytecode = File;
    fn open_module(&self, name: String) -> Self::ModuleBytecode {
        assert!(!name.contains("."));
        File::open(self.libdir.join(name + ".pyc")).unwrap()
    }
}



pub struct VectorWriter {
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
    pub stdout_content: Arc<Mutex<Vec<u8>>>,
    libdir: PathBuf,
}

impl MockEnvProxy {
    pub fn new(libdir: PathBuf) -> MockEnvProxy {
        MockEnvProxy { stdout_content: Arc::new(Mutex::new(vec![])), libdir: libdir }
    }
}


impl EnvProxy for MockEnvProxy {
    type Stdout = VectorWriter;
    fn stdout(&self) -> Self::Stdout {
        VectorWriter::new(self.stdout_content.clone())
    }

    type ModuleBytecode = File;
    fn open_module(&self, name: String) -> Self::ModuleBytecode {
        assert!(!name.contains("."));
        File::open(self.libdir.join(name + ".pyc")).unwrap()
    }
}
