use std::collections::HashMap;
use std::io::Write;
use super::sandbox::EnvProxy;
use super::processor::{Processor, ProcessorError, PyResult, PyFunction};
use super::objects::{ObjectRef, ObjectContent};

macro_rules! parse_arguments {
    ( $funcname:expr, $store:expr, $args:ident, $( $argname:tt $argexpected:tt : { $($argpattern:pat => $argcode:block,)* } ),* ) => {{
        let mut args_iter = $args.iter();
        $(
            match args_iter.next() {
                None => panic!(format!("Not enough arguments for function {}: no argument for positional parameter {}.", $funcname, $argname)),
                Some(obj_ref) => {
                    match $store.deref(obj_ref).content {
                        $( $argpattern => $argcode, )*
                        ref obj => panic!(format!("Bad argument for function {}: {} should be {}, not {:?}.", $funcname, $argname, $argexpected, obj)),
                    }
                }
            }
        )*
        if let Some(_) = args_iter.next() {
            panic!(format!("Too many positional arguments for function {}.", $funcname))
        }
    }};
}

fn write_stdout<EP: EnvProxy>(processor: &mut Processor<EP>, args: Vec<ObjectRef>) -> Result<PyResult, ProcessorError> {
    parse_arguments!("print", processor.store, args,
        "value" "a string or an integer": {
            ObjectContent::String(ref s) => {
                processor.envproxy.stdout().write(s.clone().into_bytes().as_slice()).unwrap(); // TODO: check
            },
        }
    );
    Ok(PyResult::Return(processor.primitive_objects.none.clone()))
}


pub fn get_default_primitives<EP: EnvProxy>() -> HashMap<String, PyFunction<EP>> {
    let mut builtins: HashMap<String, PyFunction<EP>> = HashMap::new();
    builtins.insert("write_stdout".to_string(), write_stdout);
    builtins
}
