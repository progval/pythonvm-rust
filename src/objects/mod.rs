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
    Function(ObjectRef),
    PrimitiveNamespace, // __primitives__
    PrimitiveFunction(String),
    Class(Option<ObjectRef>),
    OtherObject,
}

#[derive(Debug)]
#[derive(Clone)]
pub struct Object {
    pub name: Option<String>,
    pub content: ObjectContent,
    pub class: ObjectRef,
    pub bases: Option<Vec<ObjectRef>>, // superclasses
}

impl Object {
    pub fn new_instance(name: Option<String>, class: ObjectRef, content: ObjectContent) -> Object {
        Object { name: name, content: content, class: class, bases: None }
    }

    pub fn new_class(name: String, code: Option<ObjectRef>, metaclass: ObjectRef, bases: Vec<ObjectRef>) -> Object {
        Object { name: Some(name), content: ObjectContent::Class(code), class: metaclass, bases: Some(bases) }
    }
}

static CURRENT_REF_ID: AtomicUsize = ATOMIC_USIZE_INIT;

#[derive(Debug)]
#[derive(Hash)]
#[derive(Clone)]
#[derive(Eq)]
#[derive(PartialEq)]
pub struct ObjectRef {
    id: usize,
}

impl ObjectRef {
    // TODO: make it private
    pub fn new() -> ObjectRef {
        ObjectRef { id: CURRENT_REF_ID.fetch_add(1, Ordering::SeqCst) }
    }

    /// Like Python's is operator: reference equality
    pub fn is(&self, other: &ObjectRef) -> bool {
        return self.id == other.id
    }
}


pub struct ObjectStore {
    all_objects: HashMap<ObjectRef, Object>,
}

impl fmt::Debug for ObjectStore {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(write!(f, "ObjectStore {{ all_objects: HashMap {{\n"));
        for (ref obj_ref, ref obj) in self.all_objects.iter() {
            try!(write!(f, "\t{} => {:?}\n", obj_ref.id, obj));
        }
        write!(f, "}}}}\n")
    }
}

impl ObjectStore {
    pub fn new() -> ObjectStore {
        ObjectStore { all_objects: HashMap::new() }
    }

    pub fn allocate(&mut self, obj: Object) -> ObjectRef {
        let obj_ref = ObjectRef::new();
        self.all_objects.insert(obj_ref.clone(), obj);
        obj_ref
    }

    pub fn allocate_at(&mut self, obj_ref: ObjectRef, obj: Object) {
        match self.all_objects.get(&obj_ref) {
            None => self.all_objects.insert(obj_ref, obj),
            _ => panic!("Already allocated"),
        };
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

pub struct PrimitiveObjects {
    pub object: ObjectRef,
    pub type_: ObjectRef,

    pub none_type: ObjectRef,
    pub none: ObjectRef,

    pub int_type: ObjectRef,
    pub bool_type: ObjectRef,
    pub true_obj: ObjectRef,
    pub false_obj: ObjectRef,

    pub tuple_type: ObjectRef,
    pub list_type: ObjectRef,

    pub set_type: ObjectRef,
    pub frozenset_type: ObjectRef,

    pub bytes_type: ObjectRef,
    pub str_type: ObjectRef,

    pub function_type: ObjectRef,
    pub code_type: ObjectRef,

    pub names_map: HashMap<String, ObjectRef>,
}

impl PrimitiveObjects {
    pub fn new(store: &mut ObjectStore) -> PrimitiveObjects {
        let obj_ref = ObjectRef::new();
        let type_ref = ObjectRef::new();
        let obj = Object { name: Some("object".to_string()), content: ObjectContent::OtherObject, bases: Some(vec![]), class: type_ref.clone() };
        let type_ = Object { name: Some("type".to_string()), content: ObjectContent::OtherObject, bases: Some(vec![obj_ref.clone()]), class: type_ref.clone() };
        store.allocate_at(obj_ref.clone(), obj);
        store.allocate_at(type_ref.clone(), type_);

        let none_type = store.allocate(Object::new_class("nonetype".to_string(), None, type_ref.clone(), vec![obj_ref.clone()]));
        let none = store.allocate(Object::new_instance(Some("None".to_string()), none_type.clone(), ObjectContent::None));

        let int_type = store.allocate(Object::new_class("int".to_string(), None, type_ref.clone(), vec![obj_ref.clone()]));
        let bool_type = store.allocate(Object::new_class("bool".to_string(), None, type_ref.clone(), vec![int_type.clone()]));
        let true_obj = store.allocate(Object::new_instance(Some("True".to_string()), bool_type.clone(), ObjectContent::True));
        let false_obj = store.allocate(Object::new_instance(Some("False".to_string()), bool_type.clone(), ObjectContent::False));

        let tuple_type = store.allocate(Object::new_class("tuple".to_string(), None, type_ref.clone(), vec![obj_ref.clone()]));
        let list_type = store.allocate(Object::new_class("list".to_string(), None, type_ref.clone(), vec![obj_ref.clone()]));
        let set_type = store.allocate(Object::new_class("set".to_string(), None, type_ref.clone(), vec![obj_ref.clone()]));
        let frozenset_type = store.allocate(Object::new_class("frozenset".to_string(), None, type_ref.clone(), vec![obj_ref.clone()]));
        let bytes_type = store.allocate(Object::new_class("bytes".to_string(), None, type_ref.clone(), vec![obj_ref.clone()]));
        let str_type = store.allocate(Object::new_class("str".to_string(), None, type_ref.clone(), vec![obj_ref.clone()]));

        let function_type = store.allocate(Object::new_class("function".to_string(), None, type_ref.clone(), vec![obj_ref.clone()]));
        let code_type = store.allocate(Object::new_class("code".to_string(), None, type_ref.clone(), vec![obj_ref.clone()]));

        let mut map = HashMap::new();
        map.insert("object".to_string(), obj_ref.clone());
        map.insert("tuple".to_string(), type_ref.clone());
        map.insert("nonetype".to_string(), none_type.clone());
        map.insert("None".to_string(), none.clone());
        map.insert("True".to_string(), true_obj.clone());
        map.insert("False".to_string(), false_obj.clone());
        map.insert("int".to_string(), int_type.clone());
        map.insert("bool".to_string(), bool_type.clone());
        map.insert("tuple".to_string(), tuple_type.clone());
        map.insert("list".to_string(), list_type.clone());
        map.insert("set".to_string(), set_type.clone());
        map.insert("frozenset".to_string(), frozenset_type.clone());
        map.insert("bytes".to_string(), bytes_type.clone());
        map.insert("str".to_string(), str_type.clone());
        map.insert("function".to_string(), function_type.clone());
        map.insert("code".to_string(), code_type.clone());

        PrimitiveObjects {
            object: obj_ref, type_: type_ref,
            none_type: none_type, none: none,
            int_type: int_type, bool_type: bool_type, true_obj: true_obj, false_obj: false_obj,
            tuple_type: tuple_type, list_type: list_type,
            set_type: set_type, frozenset_type: frozenset_type,
            bytes_type: bytes_type, str_type: str_type,
            function_type: function_type, code_type: code_type,
            names_map: map,
        }
    }

    pub fn new_int(&self, i: u32) -> Object {
        Object::new_instance(None, self.int_type.clone(), ObjectContent::Int(i))
    }
    pub fn new_string(&self, s: String) -> Object {
        Object::new_instance(None, self.str_type.clone(), ObjectContent::String(s))
    }
    pub fn new_bytes(&self, b: Vec<u8>) -> Object {
        Object::new_instance(None, self.bytes_type.clone(), ObjectContent::Bytes(b))
    }
    pub fn new_tuple(&self, v: Vec<ObjectRef>) -> Object {
        Object::new_instance(None, self.tuple_type.clone(), ObjectContent::Tuple(v))
    }
    pub fn new_list(&self, v: Vec<ObjectRef>) -> Object {
        Object::new_instance(None, self.list_type.clone(), ObjectContent::List(v))
    }
    pub fn new_set(&self, v: Vec<ObjectRef>) -> Object {
        Object::new_instance(None, self.set_type.clone(), ObjectContent::Set(v))
    }
    pub fn new_frozenset(&self, v: Vec<ObjectRef>) -> Object {
        Object::new_instance(None, self.frozenset_type.clone(), ObjectContent::FrozenSet(v))
    }
    pub fn new_code(&self, c: Code) -> Object {
        Object::new_instance(None, self.code_type.clone(), ObjectContent::Code(Box::new(c)))
    }
}
