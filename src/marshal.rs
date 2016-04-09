use std::fmt;
use std::io;
use std::sync::Arc;

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

#[derive(PartialEq)]
#[derive(Debug)]
pub enum Object {
    //Null,
    None,
    False,
    True,
    //StopIter,
    //Ellipsis,
    Int(u32),
    //Float,
    //BinaryFloat,
    //Complex,
    //BinaryComplex,
    String(::std::string::String),
    //Interned,
    //Ref_,
    Tuple(Vec<Arc<Object>>),
    //List,
    //Dict,
    Code(Arc<Object>),
    //Unknown,
    //Set,
    //FrozenSet,
    //Ref,

    Bytes(Vec<u8>), // aka. ASCII in CPython's marshal
    //ShortAscii,
    //ShortAsciiInterned
}

pub enum Opcode {
    // todo
    ReturnValue,
    LoadConst,
}

macro_rules! read_byte {
    ( $r:expr ) => {{
        let mut buf = [0];
        match $r.read_exact(&mut buf) {
            Err(err) => return Err(UnmarshalError::Io(err)),
            Ok(()) => buf[0]
        }
    }};
}

fn read_long<R: io::Read>(r: &mut R) -> Result<u32, UnmarshalError> {
    let mut buf = [0, 0, 0, 0];
    match r.read_exact(&mut buf) {
        Err(err) => return Err(UnmarshalError::Io(err)),
        Ok(()) => Ok(buf[0] as u32 + 256*(buf[1] as u32 + 256*(buf[2] as u32 + 256*(buf[3] as u32))))
    }
}

fn read_ascii_string<R: io::Read>(r: &mut R, size: usize) -> Result<String, UnmarshalError> {
    let mut buf = Vec::<u8>::new();
    buf.resize(size, 0);
    match r.read_exact(&mut buf) {
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

fn read_unicode_string<R: io::Read>(r: &mut R, size: usize) -> Result<String, UnmarshalError> {
    let mut buf = Vec::<u8>::new();
    buf.resize(size, 0);
    match r.read_exact(&mut buf) {
        Err(err) => return Err(UnmarshalError::Io(err)),
        Ok(()) => ()
    };
    match String::from_utf8(buf) {
        Err(err) => return Err(UnmarshalError::Decoding(err)),
        Ok(s) => Ok(s)
    }
}

fn read_tuple<R: io::Read>(r: &mut R, references: &mut Vec<Arc<Object>>, size: usize) -> Result<Vec<Arc<Object>>, UnmarshalError> {
    let mut vector = Vec::<Arc<Object>>::new();
    vector.reserve(size);
    for _ in 0..size {
        vector.push(try!(read_object(r, references)))
    };
    Ok(vector)
}

pub fn read_object<R: io::Read>(r: &mut R, references: &mut Vec<Arc<Object>>) -> Result<Arc<Object>, UnmarshalError> {
    let byte = read_byte!(r);
    let flag = byte & 0b10000000;
    let opcode = byte & 0b01111111;
    let (add_ref, object_arc) = match opcode as char {
        '0' => return Err(UnmarshalError::UnexpectedCode("NULL object in marshal data for object".to_string())),
        'N' => (false, Arc::new(Object::None)),
        'F' => (false, Arc::new(Object::False)),
        'T' => (false, Arc::new(Object::True)),
        'i' => (true, Arc::new(Object::Int(try!(read_long(r))))),
        'z' | 'Z' => { // “short ascii”, “short ascii interned”
            let size = read_byte!(r) as usize;
            (true, Arc::new(Object::String(try!(read_ascii_string(r, size)))))
        },
        'u' => { // “unicode”
            let size = try!(read_long(r)) as usize; // TODO: overflow check if usize is smaller than u32
            (true, Arc::new(Object::String(try!(read_unicode_string(r, size)))))
        }
        's' => { // “string”, but actually bytes
            let size = try!(read_long(r)) as usize; // TODO: overflow check if usize is smaller than u32
            let mut buf = Vec::<u8>::new();
            buf.resize(size, 0);
            match r.read_exact(&mut buf) {
                Err(err) => return Err(UnmarshalError::Io(err)),
                Ok(()) => ()
            };
            (true, Arc::new(Object::Bytes(buf)))
        },
        ')' => { // “small tuple”
            let size = read_byte!(r) as usize;
            (true, Arc::new(Object::Tuple(try!(read_tuple(r, references, size)))))
        },
        '(' => { // “tuple”
            let size = try!(read_long(r)) as usize; // TODO: overflow check if usize is smaller than u32
            (true, Arc::new(Object::Tuple(try!(read_tuple(r, references, size)))))
        },
        'r' => {
            let index = try!(read_long(r)) as usize; // TODO: overflow check if usize is smaller than u32
            (false, try!(references.get(index).ok_or(UnmarshalError::InvalidReference)).clone())
        },

        _ => panic!(format!("Unsupported opcode: {}", opcode as char)),
    };
    if flag == 0 || !add_ref {
        Ok(object_arc)
    } else {
        references.push(object_arc.clone());
        Ok(object_arc)
    }
}

macro_rules! assert_unmarshal {
    ( $obj:expr, $bytecode:expr) => {{
        let mut reader: &[u8] = $bytecode;
        assert_eq!(Arc::new($obj), read_object(&mut reader, &mut Vec::new()).unwrap());
    }}
}

#[test]
fn test_basics() {
    assert_unmarshal!(Object::None, b"N");

    assert_unmarshal!(Object::True, b"T");

    assert_unmarshal!(Object::False, b"F");
}

#[test]
fn test_int() {
    assert_unmarshal!(Object::Int(0), b"\xe9\x00\x00\x00\x00");

    assert_unmarshal!(Object::Int(5), b"\xe9\x05\x00\x00\x00");

    assert_unmarshal!(Object::Int(1000), b"\xe9\xe8\x03\x00\x00");
}

#[test]
fn test_string() {
    assert_unmarshal!(Object::String("foo".to_string()), b"\xda\x03foo");

    // Note: this string was not generated with the marshal module
    assert_unmarshal!(Object::String("fooé".to_string()), b"\xda\x04foo\xe9");

    assert_unmarshal!(Object::String("fooé".to_string()), b"\xf5\x05\x00\x00\x00foo\xc3\xa9");
}

#[test]
fn test_bytes() {
    assert_unmarshal!(Object::Bytes(vec!['f' as u8, 'o' as u8, 5]), b"\xf3\x03\x00\x00\x00fo\x05");
}

#[test]
fn test_references() {
    let item = Arc::new(Object::String("foo".to_string()));
    assert_unmarshal!(Object::Tuple(vec![item.clone(), item.clone()]), b")\x02\xda\x03foor\x00\x00\x00\x00")
}
