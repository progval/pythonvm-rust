use std::fmt;
use std::io;

use super::super::objects::{Code, ObjectContent, ObjectRef, ObjectStore, PrimitiveObjects};

#[derive(Debug)]
pub enum UnmarshalError {
    Io(io::Error),
    Decoding(::std::string::FromUtf8Error),
    UnexpectedCode(String),
    InvalidReference,
}

impl fmt::Display for UnmarshalError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            UnmarshalError::Io(ref e) => write!(f, "I/O error:").and_then(|_| e.fmt(f)),
            UnmarshalError::Decoding(ref e) => write!(f, "Decoding error:").and_then(|_| e.fmt(f)),
            UnmarshalError::UnexpectedCode(ref s) => write!(f, "{}", s),
            UnmarshalError::InvalidReference => write!(f, "Invalid reference"),
        }
    }
}


macro_rules! read_byte {
    ( $reader:expr ) => {{
        let mut buf = [0];
        match $reader.read_exact(&mut buf) {
            Err(err) => return Err(UnmarshalError::Io(err)),
            Ok(()) => buf[0]
        }
    }};
}

/// Read a “marshal long”, ie. little-endian u32.
fn read_long<R: io::Read>(reader: &mut R) -> Result<u32, UnmarshalError> {
    let mut buf = [0, 0, 0, 0];
    match reader.read_exact(&mut buf) {
        Err(err) => return Err(UnmarshalError::Io(err)),
        Ok(()) => Ok(buf[0] as u32 + 256*(buf[1] as u32 + 256*(buf[2] as u32 + 256*(buf[3] as u32))))
    }
}

/// Read a string containing only ascii characters.
fn read_ascii_string<R: io::Read>(reader: &mut R, size: usize) -> Result<String, UnmarshalError> {
    let mut buf = Vec::<u8>::new();
    buf.resize(size, 0);
    match reader.read_exact(&mut buf) {
        Err(err) => return Err(UnmarshalError::Io(err)),
        Ok(()) => ()
    };
    let mut string = String::new();
    string.reserve(buf.len()); // The string will use more bytes than this if there are extended ascii characters, but it can't hurt to reserve anyway
    for c in buf {
        string.push(c as char);
    }
    Ok(string)
}

/// Read a UTF8 string
fn read_unicode_string<R: io::Read>(reader: &mut R, size: usize) -> Result<String, UnmarshalError> {
    let mut buf = Vec::<u8>::new();
    buf.resize(size, 0);
    match reader.read_exact(&mut buf) {
        Err(err) => return Err(UnmarshalError::Io(err)),
        Ok(()) => ()
    };
    match String::from_utf8(buf) {
        Err(err) => return Err(UnmarshalError::Decoding(err)),
        Ok(s) => Ok(s)
    }
}

/// Read an arbitrary number of contiguous marshal objects
fn read_objects<R: io::Read>(reader: &mut R, store: &mut ObjectStore, primitive_objects: &PrimitiveObjects, references: &mut Vec<ObjectRef>, size: usize) -> Result<Vec<ObjectRef>, UnmarshalError> {
    let mut vector = Vec::<ObjectRef>::new();
    vector.reserve(size);
    for _ in 0..size {
        let object = try!(read_object(reader, store, primitive_objects, references));
        vector.push(object);
    };
    Ok(vector)
}

macro_rules! deref_checktype {
    ( $expected:ident, $var:expr, $store:ident, $error:expr ) => {
        match $store.deref($var).content {
            ObjectContent::$expected(ref v) => v.clone(),
            _ => panic!($error),
        }
    };
    ( $expected_container:ident < $expected_content:path >, $var:expr, $store:ident, $error:expr ) => {{
        let container = deref_checktype!($expected_container, $var, $store, $error);
        let mut new_container = Vec::new();
        new_container.reserve(container.len());
        for item_ref in container.into_iter() {
            match $store.deref(&item_ref).content {
                $expected_content(ref v) => new_container.push(v.clone()),
                _ => panic!($error),
            }
        };
        new_container
    }};
}

/// Read marshal objects and build an other object containing them.
/// If the flag is true, add this object to the vector of objects before reading its content
/// (required, as the order of objects matter for references).
macro_rules! build_container {
    ( $reader:expr, $store:ident, $references:ident, $primitive_objects:ident, $factory:ident, $size:expr, $flag:expr) => {{
        if $flag {
            let obj_ref = ObjectRef::new();
            $references.push(obj_ref.clone());
            let objects = try!(read_objects($reader, $store, $primitive_objects, $references, $size));
            $store.allocate_at(obj_ref.clone(), $primitive_objects.$factory(objects));
            Ok(obj_ref)
        }
        else {
            let objects = try!(read_objects($reader, $store, $primitive_objects, $references, $size));
            Ok($store.allocate($primitive_objects.$factory(objects)))
        }
    }}
}

/// Read an tmporary marshal object, whose type is known from the first byte.
/// If it is a container, read its content too.
/// If the first bit is 1 and the marshal protocol allows the type to be referenced,
/// add it to the list of references too.
pub fn read_object<R: io::Read>(reader: &mut R, store: &mut ObjectStore, primitive_objects: &PrimitiveObjects, references: &mut Vec<ObjectRef>) -> Result<ObjectRef, UnmarshalError> {
    let byte = read_byte!(reader);
    let flag = byte & 0b10000000 != 0;
    let opcode = byte & 0b01111111;
    match opcode as char {
        '0' => return Err(UnmarshalError::UnexpectedCode("NULL object in marshal data for object".to_string())),
        'N' => Ok(primitive_objects.none.clone()),
        'F' => Ok(primitive_objects.false_obj.clone()),
        'T' => Ok(primitive_objects.true_obj.clone()),
        'i' => {
            let obj_ref = store.allocate(primitive_objects.new_int(try!(read_long(reader))));
            if flag {
                references.push(obj_ref.clone());
            }
            Ok(obj_ref)
        },
        'z' | 'Z' => { // “short ascii”, “short ascii interned”
            let size = read_byte!(reader) as usize;
            let obj_ref = store.allocate(primitive_objects.new_string(try!(read_ascii_string(reader, size))));
            if flag {
                references.push(obj_ref.clone());
            }
            Ok(obj_ref)
        },
        'u' => { // “unicode”
            let size = try!(read_long(reader)) as usize; // TODO: overflow check if usize is smaller than u32
            let obj_ref = store.allocate(primitive_objects.new_string(try!(read_unicode_string(reader, size))));
            if flag {
                references.push(obj_ref.clone());
            }
            Ok(obj_ref)
        }
        's' => { // “string”, but actually bytes
            let size = try!(read_long(reader)) as usize; // TODO: overflow check if usize is smaller than u32
            let mut buf = Vec::<u8>::new();
            buf.resize(size, 0);
            match reader.read_exact(&mut buf) {
                Err(err) => return Err(UnmarshalError::Io(err)),
                Ok(()) => ()
            };
            let obj_ref = store.allocate(primitive_objects.new_bytes(buf));
            if flag {
                references.push(obj_ref.clone());
            }
            Ok(obj_ref)
        },
        ')' => { // “small tuple”
            let size = read_byte!(reader) as usize;
            build_container!(reader, store, references, primitive_objects, new_tuple, size, flag)
        },
        '(' => { // “tuple”
            let size = try!(read_long(reader)) as usize; // TODO: overflow check if usize is smaller than u32
            build_container!(reader, store, references, primitive_objects, new_tuple, size, flag)
        },
        '[' => { // “list”
            let size = try!(read_long(reader)) as usize; // TODO: overflow check if usize is smaller than u32
            build_container!(reader, store, references, primitive_objects, new_list, size, flag)
        }
        '<' => { // “set”
            let size = try!(read_long(reader)) as usize; // TODO: overflow check if usize is smaller than u32
            build_container!(reader, store, references, primitive_objects, new_set, size, flag)
        }
        '>' => { // “frozenset”
            let size = try!(read_long(reader)) as usize; // TODO: overflow check if usize is smaller than u32
            build_container!(reader, store, references, primitive_objects, new_frozenset, size, false)
        }
        'r' => {
            let index = try!(read_long(reader));
            assert!((index as usize) < references.len());
            Ok(references.get(index as usize).unwrap().clone())
        },
        'c' => { // “code”
            let allocate_at = if flag {
                let obj_ref = ObjectRef::new();
                references.push(obj_ref.clone());
                Some(obj_ref)
            }
            else {
                None
            };
            let argcount = try!(read_long(reader));
            let kwonlyargcount = try!(read_long(reader));
            let nlocals = try!(read_long(reader));
            let stacksize = try!(read_long(reader));
            let flags = try!(read_long(reader));
            let code = try!(read_object(reader, store, primitive_objects, references));
            let consts = try!(read_object(reader, store, primitive_objects, references));
            let names = try!(read_object(reader, store, primitive_objects, references));
            let varnames = try!(read_object(reader, store, primitive_objects, references));
            let freevars = try!(read_object(reader, store, primitive_objects, references));
            let cellvars = try!(read_object(reader, store, primitive_objects, references));
            let filename = try!(read_object(reader, store, primitive_objects, references));
            let name = try!(read_object(reader, store, primitive_objects, references));
            let firstlineno = try!(read_long(reader));
            let lnotab = try!(read_object(reader, store, primitive_objects, references)); // TODO: decode this
            let code = Code {
                argcount: argcount as usize,
                kwonlyargcount: kwonlyargcount as usize,
                nlocals: nlocals,
                stacksize: stacksize,
                flags: flags,
                code: deref_checktype!(Bytes, &code, store, "Code.code must be a Bytes objet"),
                consts: deref_checktype!(Tuple, &consts, store, "Code.consts must be a Tuple objet"),
                names: deref_checktype!(Tuple < ObjectContent::String>, &names, store, "Code.names must be a Tuple objet"),
                varnames: deref_checktype!(Tuple<ObjectContent::String>, &varnames, store, "Code.varnames must be a Tuple objet"),
                freevars: deref_checktype!(Tuple, &freevars, store, "Code.freevars must be a Tuple objet"),
                cellvars: deref_checktype!(Tuple, &cellvars, store, "Code.cellvars must be a Tuple objet"),
                filename: deref_checktype!(String, &filename, store, "Code.filename must be a String objet"),
                name: deref_checktype!(String, &name, store, "Code.filename must be a String objet"),
                firstlineno: firstlineno,
                lnotab: lnotab,
            };

            let obj = primitive_objects.new_code(code);
            let obj_ref = match allocate_at {
                None => store.allocate(obj),
                Some(obj_ref) => {
                    store.allocate_at(obj_ref.clone(), obj);
                    obj_ref
                },
            };
            Ok(obj_ref)
        },

        _ => panic!(format!("Unsupported opcode: {}", opcode as char)),
    }
}

macro_rules! get_obj {
    ( $store:ident, $bytecode:expr ) => {{
        let mut reader: &[u8] = $bytecode;
        let mut refs = Vec::new();
        let primitive_objects = PrimitiveObjects::new(&mut $store);
        let obj_ref = read_object(&mut reader, &mut $store, &primitive_objects, &mut refs).unwrap();
        $store.deref(&obj_ref).content.clone()
    }};
}

macro_rules! assert_unmarshal {
    ( $expected_obj:expr, $store:ident, $bytecode:expr) => {{
        $store = ObjectStore::new();
        let ref obj = get_obj!($store, $bytecode);
        assert_eq!($expected_obj, *obj);
    }};
}

#[test]
fn test_basics() {
    let mut store;

    assert_unmarshal!(ObjectContent::None, store, b"N");

    assert_unmarshal!(ObjectContent::True, store, b"T");

    assert_unmarshal!(ObjectContent::False, store, b"F");
}


#[test]
fn test_int() {
    let mut store;

    assert_unmarshal!(ObjectContent::Int(0), store, b"\xe9\x00\x00\x00\x00");

    assert_unmarshal!(ObjectContent::Int(5), store, b"\xe9\x05\x00\x00\x00");

    assert_unmarshal!(ObjectContent::Int(1000), store, b"\xe9\xe8\x03\x00\x00");
}

#[test]
fn test_string() {
    let mut store;

    assert_unmarshal!(ObjectContent::String("foo".to_string()), store, b"\xda\x03foo");

    // Note: this string was not generated with the marshal module
    assert_unmarshal!(ObjectContent::String("fooé".to_string()), store, b"\xda\x04foo\xe9");

    assert_unmarshal!(ObjectContent::String("fooé".to_string()), store, b"\xf5\x05\x00\x00\x00foo\xc3\xa9");
}
#[test]
fn test_bytes() {
    let mut store;

    assert_unmarshal!(ObjectContent::Bytes(vec!['f' as u8, 'o' as u8, 5]), store, b"\xf3\x03\x00\x00\x00fo\x05");
}

#[test]
fn test_references() {
    let mut store = ObjectStore::new();
    let ref obj = get_obj!(store, b")\x02\xda\x03foor\x00\x00\x00\x00");
    match *obj {
        ObjectContent::Tuple(ref v) => {
            assert_eq!(v.len(), 2);
            let o1 = store.deref(v.get(0).unwrap()).content.clone();
            let o2 = store.deref(v.get(1).unwrap()).content.clone();
            match (o1, o2) {
                (ObjectContent::String(s1), ObjectContent::String(s2)) => {
                    assert_eq!(s1, "foo".to_string());
                    assert_eq!(s2, "foo".to_string());
                }
                _ => panic!("Not strings"),
            };
        },
        _ => panic!("Not tuple."),
    }
}


#[test]
fn test_recursive_reference() {
    let mut store = ObjectStore::new();
    // l = []; l.append(l)
    let ref obj = get_obj!(store, b"\xdb\x01\x00\x00\x00r\x00\x00\x00\x00'");
    match *obj {
        ObjectContent::List(ref v) => {
            assert_eq!(v.len(), 1);
            let obj2 = store.deref(v.get(0).unwrap()).content.clone();
            assert_eq!(*obj, obj2)
        },
        _ => panic!("Not list."),
    }
}

#[test]
fn test_code() {
    // >>> def foo(bar):
    // ...     return bar
    let mut store = ObjectStore::new();
    let obj = get_obj!(store,
        b"\xe3\x01\x00\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x01\x00\x00\x00C\x00\x00\x00s\x04\x00\x00\x00|\x00\x00S)\x01N\xa9\x00)\x01Z\x03barr\x01\x00\x00\x00r\x01\x00\x00\x00\xfa\x07<stdin>\xda\x03foo\x01\x00\x00\x00s\x02\x00\x00\x00\x00\x01");
    match obj {
        ObjectContent::Code(ref code) => {
            assert_eq!(code.argcount, 1);
            assert_eq!(code.code, vec![124, 0, 0, 83]);
            assert_eq!(code.filename, "<stdin>".to_string());
            assert_eq!(code.consts.len(), 1);
            assert_eq!(store.deref(code.consts.get(0).unwrap()).content, ObjectContent::None);
        },
        _ => panic!("Not code"),
    }
}

#[test]
fn test_module() {
    let bytes: &[u8] = b"\xe3\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x00\x00@\x00\x00\x00s\x10\x00\x00\x00d\x00\x00d\x01\x00\x84\x00\x00Z\x00\x00d\x02\x00S)\x03c\x01\x00\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x02\x00\x00\x00C\x00\x00\x00s\x1e\x00\x00\x00t\x00\x00j\x01\x00|\x00\x00\x83\x01\x00\x01t\x00\x00j\x01\x00d\x01\x00\x83\x01\x00\x01d\x00\x00S)\x02N\xda\x01\n)\x02Z\x0e__primitives__Z\x0cwrite_stdout)\x01\xda\x05value\xa9\x00r\x03\x00\x00\x00\xfa\x15pythonlib/builtins.py\xda\x05print\x01\x00\x00\x00s\x04\x00\x00\x00\x00\x01\r\x01r\x05\x00\x00\x00N)\x01r\x05\x00\x00\x00r\x03\x00\x00\x00r\x03\x00\x00\x00r\x03\x00\x00\x00r\x04\x00\x00\x00\xda\x08<module>\x01\x00\x00\x00s\x00\x00\x00\x00";
    let mut store = ObjectStore::new();
    get_obj!(store, bytes);
}
