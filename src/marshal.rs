use std::fmt;
use std::io;

#[derive(Debug)]
pub enum UnmarshalError {
    Io(io::Error),
    UnexpectedCode(String),
}

impl fmt::Display for UnmarshalError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            UnmarshalError::Io(ref e) => write!(f, "I/O error:").and_then(|_| e.fmt(f)),
            UnmarshalError::UnexpectedCode(ref s) => write!(f, "{}", s),
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
    //Tuple,
    //List,
    //Dict,
    //Code,
    //Unicode,
    //Unknown,
    //Set,
    //FrozenSet,
    //Ref,

    Bytes(Vec<u8>), // aka. ASCII in CPython's marshal
    //AsciiInterned,
    //SmallTuple,
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

pub fn read_object<R: io::Read>(r: &mut R) -> Result<Object, UnmarshalError> {
    let byte = read_byte!(r);
    let _flag = byte & 0b10000000; // TODO: do something with this
    let opcode = byte & 0b01111111;
    let object = match opcode as char {
        '0' => return Err(UnmarshalError::UnexpectedCode("NULL object in marshal data for object".to_string())),
        'N' => Object::None,
        'F' => Object::False,
        'T' => Object::True,
        'i' => Object::Int(try!(read_long(r))),
        'z' | 'Z' => { // “short ascii”, “short ascii interned”
            let size = read_byte!(r) as usize;
            Object::String(try!(read_ascii_string(r, size)))
        },
        _ => panic!(format!("Unsupported opcode: {}", opcode as char)),
    };
    Ok(object)
}

#[test]
fn test_basics() {
    let mut reader: &[u8] = b"N";
    assert_eq!(Object::None, read_object(&mut reader).unwrap());

    let mut reader: &[u8] = b"T";
    assert_eq!(Object::True, read_object(&mut reader).unwrap());

    let mut reader: &[u8] = b"F";
    assert_eq!(Object::False, read_object(&mut reader).unwrap());
}

#[test]
fn test_int() {
    let mut reader: &[u8] = b"\xe9\x00\x00\x00\x00";
    assert_eq!(Object::Int(0), read_object(&mut reader).unwrap());

    let mut reader: &[u8] = b"\xe9\x05\x00\x00\x00";
    assert_eq!(Object::Int(5), read_object(&mut reader).unwrap());

    let mut reader: &[u8] = b"\xe9\xe8\x03\x00\x00";
    assert_eq!(Object::Int(1000), read_object(&mut reader).unwrap());
}

#[test]
fn test_string() {
    let mut reader: &[u8] = b"\xda\x03foo";
    assert_eq!(Object::String("foo".to_string()), read_object(&mut reader).unwrap());

    let mut reader: &[u8] = b"\xda\x04foo\xe9"; // Note: this string was not generated with the marshal module
    assert_eq!(Object::String("fooé".to_string()), read_object(&mut reader).unwrap());
}
