extern crate pythonvm;

use std::env::args;
use std::fs::File;
use std::path::PathBuf;

fn parse_args() -> (String, Option<(String, String)>) {
    let mut args = args();
    let executable = args.next().unwrap();
    let libdir = args.next();
    let filename = args.next();
    let extra = args.next();
    match (libdir, filename, extra) {
        (Some(libdir), Some(filename), None) => (executable, Some((libdir, filename))),
        _ => (executable, None),
    }
}


pub fn main() {
    let (libdir, filename) = match parse_args() {
        (_, Some((libdir, filename))) => (libdir, filename),
        (executable, None) => {
            println!("Syntax: {} pythonlib/ filename.pyc", executable);
            return
        }
    };
    let mut file = File::open(filename).unwrap();
    let mut path = PathBuf::new();
    path.push(&libdir);
    let env_proxy = pythonvm::RealEnvProxy::new(path);
    let (_processor, _result) = pythonvm::run_file(&mut file, env_proxy).unwrap();
}
