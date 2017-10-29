extern crate pythonvm;

use std::path::PathBuf;
use std::env;
use pythonvm::{MockEnvProxy, PyResult, run_file};

#[test]
fn test_hello_world() {
    let mut reader: &[u8] = b"3\r\r\n\xe1\xc8\xf4Y\x15\x00\x00\x00\xe3\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x00\x00@\x00\x00\x00s\x0c\x00\x00\x00e\x00d\x00\x83\x01\x01\x00d\x01S\x00)\x02z\x0bhello worldN)\x01\xda\x05print\xa9\x00r\x02\x00\x00\x00r\x02\x00\x00\x00\xfa\x16examples/helloworld.py\xda\x08<module>\x01\x00\x00\x00s\x00\x00\x00\x00";
    let mut path = PathBuf::new();
    path.push(env::current_dir().unwrap());
    path.push("pythonlib/");
    let envproxy = MockEnvProxy::new(path);
    let (processor, result) = run_file(&mut reader, envproxy).unwrap();
    if let PyResult::Return(_) = result {
        assert_eq!(*processor.envproxy.stdout_content.lock().unwrap(), b"hello world\n");
    }
    else {
        panic!(format!("Exited with: {:?}", result))
    }
}
