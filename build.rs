use std::process::Command;

fn main() {
    Command::new("/usr/bin/env")
            .arg("python3")
            .arg("-m")
            .arg("compileall")
            .arg("-b") // old-style bytecode layout
            .arg("pythonlib/")
            .arg("examples/")
            .spawn()
            .unwrap()
            .wait()
            .unwrap();
}
