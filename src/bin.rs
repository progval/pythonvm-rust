extern crate pythonvm;

use std::env::args;
use std::fs::File;

fn parse_args() -> (String, Option<String>) {
    let mut args = args();
    let executable = args.next().unwrap();
    let filename = args.next();
    let extra = args.next();
    match (filename, extra) {
        (Some(filename), None) => (executable, Some(filename)),
        _ => (executable, None),
    }
}


pub fn main() {
    let filename = match parse_args() {
        (_, Some(filename)) => filename,
        (executable, None) => {
            println!("Syntax: {} filename.pyc", executable);
            return
        }
    };
    let mut file = File::open(filename).unwrap();
    let env_proxy = pythonvm::RealEnvProxy::new();
    let (_processor, _result) = pythonvm::run_module(&mut file, env_proxy).unwrap();
}
