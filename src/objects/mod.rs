use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};
use std::fmt;

#[derive(Debug)]
#[derive(Clone)]
#[derive(PartialEq)]
#[derive(Eq)]
pub struct Code {
    pub argcount: usize,
    pub kwonlyargcount: u32,
    pub nlocals: u32,
    pub stacksize: u32,
    pub flags: u32,
    pub code: Vec<u8>,
    pub consts: Vec<ObjectRef>,
    pub names: Vec<String>,
    pub varnames: Vec<String>,
    pub freevars: Vec<ObjectRef>,
    pub cellvars: Vec<ObjectRef>,
    pub filename: String,
    pub name: String,
    pub firstlineno: u32,
    pub lnotab: ObjectRef,
}

impl Code {
    pub fn co_varargs(&self) -> bool {
        self.flags & 0x4 != 0
    }
}

#[derive(Debug)]
#[derive(Clone)]
#[derive(PartialEq)]
#[derive(Eq)]
pub enum ObjectContent {
    Hole, // Temporary object
    None,
    True,
    False,
    Int(u32),
    String(::std::string::String),
    Tuple(Vec<ObjectRef>),
    List(Vec<ObjectRef>),
    Code(Box<Code>),
    Set(Vec<ObjectRef>),
    FrozenSet(Vec<ObjectRef>),
    Bytes(Vec<u8>),
    Function(String, ObjectRef), // name, code
    PrimitiveNamespace, // __primitives__
    PrimitiveFunction(String),
}

#[derive(Debug)]
pub struct Object {
    pub content: ObjectContent,
}

#[derive(Debug)]
#[derive(Hash)]
#[derive(Clone)]
#[derive(Eq)]
#[derive(PartialEq)]
pub struct ObjectRef {
    id: usize,
}

static CURRENT_REF_ID: AtomicUsize = ATOMIC_USIZE_INIT;

pub struct ObjectStore {
    all_objects: HashMap<ObjectRef, Object>,
}

impl fmt::Debug for ObjectStore {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ObjectStore {{ all_objects: HashMap {{\n");
        for (ref obj_ref, ref obj) in self.all_objects.iter() {
            write!(f, "\t{} => {:?}\n", obj_ref.id, obj);
        }
        write!(f, "}}}}\n")
    }
}

impl ObjectStore {
    pub fn new() -> ObjectStore {
        ObjectStore { all_objects: HashMap::new() }
    }

    pub fn allocate(&mut self, obj: ObjectContent) -> ObjectRef {
        let obj_ref = ObjectRef { id: CURRENT_REF_ID.fetch_add(1, Ordering::SeqCst) };
        self.all_objects.insert(obj_ref.clone(), Object { content: obj });
        obj_ref
    }

    pub fn replace_hole(&mut self, obj_ref: &ObjectRef, obj: ObjectContent) {
        match self.all_objects.get(obj_ref).unwrap().content {
            ObjectContent::Hole => (),
            _ => panic!("Cannot replace non-hole"),
        };
        self.all_objects.insert(obj_ref.clone(), Object { content: obj });
    }

    pub fn deref(&self, obj_ref: &ObjectRef) -> &Object {
        // TODO: check the reference is valid
        self.all_objects.get(obj_ref).unwrap()
    }
    pub fn deref_mut(&mut self, obj_ref: &ObjectRef) -> &mut Object {
        // TODO: check the reference is valid
        self.all_objects.get_mut(obj_ref).unwrap()
    }
}
