extern crate itertools;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};
use std::fmt;
use self::itertools::Itertools;
use super::state::State;
use super::sandbox::EnvProxy;

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
    Function(String, ObjectRef), // module, code
    Module(ObjectRef),
    PrimitiveNamespace, // __primitives__
    PrimitiveFunction(String),
    Class(Option<ObjectRef>),
    RandomAccessIterator(ObjectRef, usize, u64), // container, index, container version
    OtherObject,
}

static CURRENT_VERSION: AtomicUsize = ATOMIC_USIZE_INIT;

#[derive(Debug)]
#[derive(Clone)]
pub struct Object {
    pub version: u64,
    pub name: Option<String>,
    pub content: ObjectContent,
    pub class: ObjectRef,
    pub bases: Option<Vec<ObjectRef>>, // superclasses
}

impl Object {
    fn new_version() -> u64 {
        CURRENT_VERSION.fetch_add(1, Ordering::SeqCst) as u64 // TODO: avoid cast
    }
    pub fn new_instance(name: Option<String>, class: ObjectRef, content: ObjectContent) -> Object {
        Object {
            version: Object::new_version(),
            name: name,
            content: content,
            class: class,
            bases: None,
        }
    }

    pub fn new_class(name: String, code: Option<ObjectRef>, metaclass: ObjectRef, bases: Vec<ObjectRef>) -> Object {
        Object {
            version: Object::new_version(),
            name: Some(name),
            content: ObjectContent::Class(code),
            class: metaclass,
            bases: Some(bases),
        }
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

    fn repr_vec(l: &Vec<ObjectRef>, store: &ObjectStore) -> String {
        l.iter().map(|r| r.repr(store)).join(", ")
    }

    pub fn repr(&self, store: &ObjectStore) -> String {
        let obj = store.deref(self);
        match obj.content {
            ObjectContent::None => "None".to_string(),
            ObjectContent::True => "True".to_string(),
            ObjectContent::False => "False".to_string(),
            ObjectContent::Int(ref i) => i.to_string(),
            ObjectContent::Bytes(ref s) => "<bytes>".to_string(), // TODO
            ObjectContent::String(ref s) => format!("'{}'", s), // TODO: escape
            ObjectContent::Tuple(ref l) => format!("tuple({})", ObjectRef::repr_vec(l, store)),
            ObjectContent::List(ref l) => format!("[{}]", ObjectRef::repr_vec(l, store)),
            ObjectContent::Code(_) => "<code object>".to_string(),
            ObjectContent::Set(ref l) => format!("set({})", ObjectRef::repr_vec(l, store)),
            ObjectContent::FrozenSet(ref l) => format!("frozenset({})", ObjectRef::repr_vec(l, store)),
            ObjectContent::Function(ref module, ref _code) => {
                match obj.name {
                    None => format!("<anonymous function in module {}>", module),
                    Some(ref s) => format!("<function {} in module {}>", s, module),
                }
            },
            ObjectContent::PrimitiveNamespace => "__primitives__".to_string(),
            ObjectContent::PrimitiveFunction(ref s) => format!("__primitives__.{}", s),
            ObjectContent::Class(_) => {
                match obj.name {
                    None => "<anonymous class>".to_string(),
                    Some(ref s) => format!("<class {}>", s),
                }
            },
            ObjectContent::Module(ref _code) => {
                match obj.name {
                    None => "<anonymous module>".to_string(),
                    Some(ref s) => format!("<module {}", s),
                }
            },
            ObjectContent::RandomAccessIterator(ref container, ref index, ref version) => {
                format!("<iterator on {} at index {}>", store.deref(container).class.repr(store), version)
            }
            ObjectContent::OtherObject => format!("<{} instance>", obj.class.repr(store)),
        }
    }

    pub fn module(&self, store: &ObjectStore) -> String {
        let func = store.deref(self);
        let ref name = func.name;
        match func.content {
            ObjectContent::Function(ref module_name, ref _code) => module_name.clone(),
            ObjectContent::Module(ref _code) => name.clone().unwrap(),
            _ => panic!(format!("Not a function/module: {:?}", func)),
        }
    }

    pub fn iter<EP: EnvProxy>(&self, state: &mut State<EP>) -> ObjectRef {
        // TODO: check it's a list or a tuple
        let obj_version = state.store.deref(self).version;
        let iterator = Object::new_instance(None, state.primitive_objects.iterator_type.clone(), ObjectContent::RandomAccessIterator(self.clone(), 0, obj_version));
        state.store.allocate(iterator)
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

    pub iterator_type: ObjectRef,

    pub function_type: ObjectRef,
    pub code_type: ObjectRef,

    pub module: ObjectRef,

    pub baseexception: ObjectRef,
    pub processorerror: ObjectRef,
    pub exception: ObjectRef,

    pub nameerror: ObjectRef,
    pub attributeerror: ObjectRef,
    pub typeerror: ObjectRef,
    pub stopiteration: ObjectRef,

    pub lookuperror: ObjectRef,
    pub keyerror: ObjectRef,

    pub names_map: HashMap<String, ObjectRef>,
}

impl PrimitiveObjects {
    pub fn new(store: &mut ObjectStore) -> PrimitiveObjects {
        let obj_ref = ObjectRef::new();
        let type_ref = ObjectRef::new();
        let obj = Object {
            version: Object::new_version(),
            name: Some("object".to_string()),
            content: ObjectContent::OtherObject,
            bases: Some(vec![]),
            class: type_ref.clone()
        };
        let type_ = Object {
            version: Object::new_version(),
            name: Some("type".to_string()),
            content: ObjectContent::OtherObject,
            bases: Some(vec![obj_ref.clone()]),
            class: type_ref.clone()
        };
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
        let iterator_type = store.allocate(Object::new_class("iterator".to_string(), None, type_ref.clone(), vec![obj_ref.clone()]));

        let function_type = store.allocate(Object::new_class("function".to_string(), None, type_ref.clone(), vec![obj_ref.clone()]));
        let code_type = store.allocate(Object::new_class("code".to_string(), None, type_ref.clone(), vec![obj_ref.clone()]));

        let module = store.allocate(Object::new_class("module".to_string(), None, type_ref.clone(), vec![obj_ref.clone()]));

        let baseexception = store.allocate(Object::new_class("BaseException".to_string(), None, type_ref.clone(), vec![obj_ref.clone()]));
        let processorerror = store.allocate(Object::new_class("ProcessorError".to_string(), None, type_ref.clone(), vec![baseexception.clone()]));
        let exception = store.allocate(Object::new_class("Exception".to_string(), None, type_ref.clone(), vec![baseexception.clone()]));

        let nameerror = store.allocate(Object::new_class("NameError".to_string(), None, type_ref.clone(), vec![exception.clone()]));
        let attributeerror = store.allocate(Object::new_class("AttributeError".to_string(), None, type_ref.clone(), vec![exception.clone()]));
        let typeerror = store.allocate(Object::new_class("TypeError".to_string(), None, type_ref.clone(), vec![exception.clone()]));
        let stopiteration = store.allocate(Object::new_class("StopIteration".to_string(), None, type_ref.clone(), vec![exception.clone()]));

        let lookuperror = store.allocate(Object::new_class("LookupError".to_string(), None, type_ref.clone(), vec![exception.clone()]));
        let keyerror = store.allocate(Object::new_class("KeyError".to_string(), None, type_ref.clone(), vec![lookuperror.clone()]));


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
        map.insert("module".to_string(), module.clone());

        // Base classes
        map.insert("BaseException".to_string(), baseexception.clone());
        map.insert("ProcessorError".to_string(), processorerror.clone());
        map.insert("Exception".to_string(), exception.clone());

        map.insert("NameError".to_string(), nameerror.clone());
        map.insert("AttributeError".to_string(), attributeerror.clone());
        map.insert("TypeError".to_string(), typeerror.clone());
        map.insert("StopIteration".to_string(), stopiteration.clone());

        map.insert("LookupError".to_string(), lookuperror.clone());
        map.insert("KeyError".to_string(), keyerror.clone());

        PrimitiveObjects {
            object: obj_ref, type_: type_ref,
            none_type: none_type, none: none,
            int_type: int_type, bool_type: bool_type, true_obj: true_obj, false_obj: false_obj,
            tuple_type: tuple_type, list_type: list_type,
            set_type: set_type, frozenset_type: frozenset_type,
            bytes_type: bytes_type, str_type: str_type,
            iterator_type: iterator_type,
            function_type: function_type, code_type: code_type,
            baseexception: baseexception, processorerror: processorerror, exception: exception,
            nameerror: nameerror, attributeerror: attributeerror, typeerror: typeerror, stopiteration: stopiteration,
            lookuperror: lookuperror, keyerror: keyerror,
            module: module,
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
    pub fn new_function(&self, name: String, module_name: String, code: ObjectRef) -> Object {
        Object::new_instance(Some(name), self.function_type.clone(), ObjectContent::Function(module_name, code))
    }
    pub fn new_module(&self, name: String, code: ObjectRef) -> Object {
        Object::new_instance(Some(name), self.module.clone(), ObjectContent::Module(code))
    }
}
