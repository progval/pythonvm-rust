use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};

#[derive(Debug)]
#[derive(Clone)]
pub struct Code {/*
    pub argcount: u32,
    pub kwonlyargcount: u32,
    pub nlocals: u32,
    pub stacksize: u32,
    pub flags: u32,*/
    pub code: Vec<u8>,
    pub consts: Vec<ObjectRef>,
    pub names: Vec<ObjectRef>,/*
    pub varnames: Object,
    pub freevars: Object,
    pub cellvars: Object,
    pub filename: Object,
    pub name: Object,
    pub firstlineno: u32,
    pub lnotab: Object,*/
}

#[derive(Debug)]
pub enum ObjectContent {
    None,
    True,
    False,
    Int(u32),
    String(::std::string::String),
    Tuple(Vec<ObjectRef>),
    List(Vec<ObjectRef>),
    Code(Code),
    Set(Vec<ObjectRef>),
    FrozenSet(Vec<ObjectRef>),
    Bytes(Vec<u8>),
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

#[derive(Debug)]
pub struct ObjectStore {
    all_objects: HashMap<ObjectRef, Object>,
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

    pub fn deref(&self, obj_ref: &ObjectRef) -> &Object {
        // TODO: check the reference is valid
        self.all_objects.get(obj_ref).unwrap()
    }
}
