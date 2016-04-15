extern crate pythonvm;

use std::path::PathBuf;
use std::env;
use pythonvm::{MockEnvProxy, run_file};

#[test]
fn test_hello_world() {
    let mut reader: &[u8] = b"\xee\x0c\r\n\xb0\x92\x0fW\x15\x00\x00\x00\xe3\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x00\x00@\x00\x00\x00s\x0e\x00\x00\x00e\x00\x00d\x00\x00\x83\x01\x00\x01d\x01\x00S)\x02z\x0bHello worldN)\x01\xda\x05print\xa9\x00r\x02\x00\x00\x00r\x02\x00\x00\x00\xfa\x16examples/helloworld.py\xda\x08<module>\x01\x00\x00\x00s\x00\x00\x00\x00";
    let mut path = PathBuf::new();
    path.push(env::current_dir().unwrap());
    path.push("pythonlib/");
    let envproxy = MockEnvProxy::new(path);
    let (processor, _result) = run_file(&mut reader, envproxy).unwrap();
    assert_eq!(*processor.envproxy.stdout_content.lock().unwrap(), b"Hello world\n");
}
