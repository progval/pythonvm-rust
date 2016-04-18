use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::collections::linked_list::LinkedList;
use super::sandbox::EnvProxy;
use super::processor::{Processor, ProcessorError, PyResult, PyFunction};
use super::objects::{ObjectRef, ObjectContent, Object};

macro_rules! parse_first_arguments {
    ( $funcname:expr, $store:expr, $args:ident, $args_iter:ident, $( $argname:tt $argexpected:tt : { $($argpattern:pat => $argcode:block,)* } ),* ) => {{
        $(
            match $args_iter.next() {
                None => panic!(format!("Not enough arguments for function {}: no argument for positional parameter {}.", $funcname, $argname)),
                Some(obj_ref) => {
                    match $store.deref(&obj_ref).content {
                        $( $argpattern => $argcode, )*
                        ref obj => panic!(format!("Bad argument for function {}: {} should be {}, not {:?}.", $funcname, $argname, $argexpected, obj)),
                    }
                }
            }
        )*
    }};
}

macro_rules! parse_arguments {
    ( $funcname:expr, $store:expr, $args:ident, $( $argname:tt $argexpected:tt : { $($argpattern:pat => $argcode:block,)* } ),* ) => {{
        let mut args_iter = $args.iter();
        parse_first_arguments!($funcname, $store, $args, args_iter, $( $argname $argexpected : { $($argpattern => $argcode,)* } ),*);
        if let Some(_) = args_iter.next() {
            panic!(format!("Too many positional arguments for function {}.", $funcname))
        }
    }};
}

fn write_stdout<EP: EnvProxy>(processor: &mut Processor<EP>, args: Vec<ObjectRef>) -> Result<PyResult, ProcessorError> {
    parse_arguments!("__primitives__.write_stdout", processor.store, args,
        "value" "a string, boolean, or integer": {
            ObjectContent::String(ref s) => {
                processor.envproxy.stdout().write(s.clone().into_bytes().as_slice()).unwrap(); // TODO: check
            },
            ObjectContent::Int(ref i) => {
                processor.envproxy.stdout().write(i.to_string().into_bytes().as_slice()).unwrap(); // TODO: check
            },
            ObjectContent::True => {
                processor.envproxy.stdout().write(b"True").unwrap(); // TODO: check
            },
            ObjectContent::False => {
                processor.envproxy.stdout().write(b"False").unwrap(); // TODO: check
            },
        }
    );
    Ok(PyResult::Return(processor.primitive_objects.none.clone()))
}

fn build_class<EP: EnvProxy>(processor: &mut Processor<EP>, args: Vec<ObjectRef>) -> Result<PyResult, ProcessorError> {
    let name;
    let code;
    let mut args_iter = args.into_iter();
    parse_first_arguments!("__primitives__.build_class", processor.store, args, args_iter,
        "func" "a function": {
            ObjectContent::Function(ref code_arg) => {
                code = code_arg.clone();
            },
        },
        "name" "a string": {
            ObjectContent::String(ref name_arg) => { name = name_arg.clone() },
        }
    );
    let bases: Vec<ObjectRef> = args_iter.collect();
    let bases = if bases.len() == 0 {
        vec![processor.primitive_objects.object.clone()]
    }
    else {
        bases
    };
    Ok(PyResult::Return(processor.store.allocate(Object::new_class(name, Some(code), processor.primitive_objects.type_.clone(), bases))))
}

fn issubclass<EP: EnvProxy>(processor: &mut Processor<EP>, args: Vec<ObjectRef>) -> Result<PyResult, ProcessorError> {
    if args.len() != 2 {
        panic!(format!("__primitives__.issubclass takes 2 arguments, not {}", args.len()))
    }
    let first = args.get(0).unwrap();
    let second = args.get(1).unwrap();
    let mut visited = HashSet::new();
    let mut to_visit = LinkedList::new();
    to_visit.push_back(first.clone());
    while let Some(candidate) = to_visit.pop_front() {
        if !visited.insert(candidate.clone()) {
            // Already visited
            continue
        };
        if candidate.is(second) {
            return Ok(PyResult::Return(processor.primitive_objects.true_obj.clone()))
        };
        match processor.store.deref(&candidate).bases {
            None => (),
            Some(ref bases) => {
                for base in bases.iter() {
                    to_visit.push_back(base.clone())
                }
            }
        };
    }
    Ok(PyResult::Return(processor.primitive_objects.false_obj.clone()))
}

fn isinstance<EP: EnvProxy>(processor: &mut Processor<EP>, mut args: Vec<ObjectRef>) -> Result<PyResult, ProcessorError> {
    if args.len() != 2 {
        panic!(format!("__primitives__.isinstance takes 2 arguments, not {}", args.len()))
    }
    let second = args.pop().unwrap();
    let first = args.pop().unwrap();
    let new_args = vec![processor.store.deref(&first).class.clone(), second];
    issubclass(processor, new_args)
}


pub fn get_default_primitives<EP: EnvProxy>() -> HashMap<String, PyFunction<EP>> {
    let mut builtins: HashMap<String, PyFunction<EP>> = HashMap::new();
    builtins.insert("write_stdout".to_string(), write_stdout);
    builtins.insert("build_class".to_string(), build_class);
    builtins.insert("issubclass".to_string(), issubclass);
    builtins.insert("isinstance".to_string(), isinstance);
    builtins
}
