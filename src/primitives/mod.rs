use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::collections::linked_list::LinkedList;
use super::sandbox::EnvProxy;
use super::state::{State, PyResult, PyFunction, raise, return_value};
use super::objects::{ObjectRef, ObjectContent, Object, ObjectStore};
use super::processor::frame::Frame;

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

fn write_stdout<EP: EnvProxy>(processor: &mut State<EP>, call_stack: &mut Vec<Frame>, args: Vec<ObjectRef>) {
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
    return_value(call_stack, processor.primitive_objects.none.clone())
}

fn build_class<EP: EnvProxy>(processor: &mut State<EP>, call_stack: &mut Vec<Frame>, args: Vec<ObjectRef>) {
    let name;
    let code;
    let mut args_iter = args.into_iter();
    parse_first_arguments!("__primitives__.build_class", processor.store, args, args_iter,
        "func" "a function": {
            ObjectContent::Function(_, ref code_arg, _) => {
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
    return_value(call_stack, processor.store.allocate(Object::new_class(name, Some(code), processor.primitive_objects.type_.clone(), bases)))
}

pub fn native_issubclass(store: &ObjectStore, first: &ObjectRef, second: &ObjectRef) -> bool {
    let mut visited = HashSet::new();
    let mut to_visit = LinkedList::new();
    to_visit.push_back(first.clone());
    while let Some(candidate) = to_visit.pop_front() {
        if !visited.insert(candidate.clone()) {
            // Already visited
            continue
        };
        if candidate.is(second) {
            return true
        };
        match store.deref(&candidate).bases {
            None => (),
            Some(ref bases) => {
                for base in bases.iter() {
                    to_visit.push_back(base.clone())
                }
            }
        };
    }
    false
}

fn issubclass<EP: EnvProxy>(state: &mut State<EP>, call_stack: &mut Vec<Frame>, args: Vec<ObjectRef>) {
    if args.len() != 2 {
        panic!(format!("__primitives__.issubclass takes 2 arguments, not {}", args.len()))
    }
    let first = args.get(0).unwrap();
    let second = args.get(1).unwrap();
    let res = native_issubclass(&state.store, first, second);
    if res {
        return_value(call_stack, state.primitive_objects.true_obj.clone())
    }
    else {
        return_value(call_stack, state.primitive_objects.false_obj.clone())
    }
}

pub fn native_isinstance(store: &ObjectStore, first: &ObjectRef, second: &ObjectRef) -> bool {
    native_issubclass(store, &store.deref(&first).class, second)
}

fn isinstance<EP: EnvProxy>(state: &mut State<EP>, call_stack: &mut Vec<Frame>, mut args: Vec<ObjectRef>) {
    if args.len() != 2 {
        panic!(format!("__primitives__.isinstance takes 2 arguments, not {}", args.len()))
    }
    let second = args.pop().unwrap();
    let first = args.pop().unwrap();
    let res = native_isinstance(&state.store, &first, &second);
    if res {
        return_value(call_stack, state.primitive_objects.true_obj.clone())
    }
    else {
        return_value(call_stack, state.primitive_objects.false_obj.clone())
    }
}

fn iter<EP: EnvProxy>(state: &mut State<EP>, call_stack: &mut Vec<Frame>, mut args: Vec<ObjectRef>) {
    if args.len() != 1 {
        panic!(format!("__primitives__.iter takes 1 arguments, not {}", args.len()))
    }
    let iterator_ref = args.last().unwrap();
    let iterator = state.store.deref(iterator_ref).clone();
    match iterator.content {
        ObjectContent::RandomAccessIterator(container_ref, index, container_version) => {
            let value = {
                let container = state.store.deref(&container_ref);
                if container.version != container_version {
                    panic!("Container changed while iterating.")
                };
                match container.content {
                    ObjectContent::List(ref v) | ObjectContent::Tuple(ref v) => v.get(index).map(|r| r.clone()),
                    ref c => panic!(format!("RandomAccessIterator does not support {}", container_ref.repr(&state.store)))
                }
            };
            match value {
                Some(value) => {
                    let mut iterator = state.store.deref_mut(iterator_ref);
                    iterator.content = ObjectContent::RandomAccessIterator(container_ref, index+1, container_version);
                    return_value(call_stack, value.clone())
                }
                None => {
                    let stopiteration = state.primitive_objects.stopiteration.clone();

                    return raise(state, call_stack, stopiteration, "StopIteration instance".to_string())
                }
            }
        }
        ref c =>  {
            let repr = iterator_ref.repr(&state.store);
            let exc = Object::new_instance(None, state.primitive_objects.typeerror.clone(), ObjectContent::OtherObject);
            let exc = state.store.allocate(exc);
            raise(state, call_stack, exc, format!("{} is not an iterator", repr));
        }
    }
}


pub fn get_default_primitives<EP: EnvProxy>() -> HashMap<String, PyFunction<EP>> {
    let mut builtins: HashMap<String, PyFunction<EP>> = HashMap::new();
    builtins.insert("write_stdout".to_string(), write_stdout);
    builtins.insert("build_class".to_string(), build_class);
    builtins.insert("issubclass".to_string(), issubclass);
    builtins.insert("isinstance".to_string(), isinstance);
    builtins.insert("iter".to_string(), iter);
    builtins
}
