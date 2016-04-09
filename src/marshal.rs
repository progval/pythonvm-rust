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
    //String,
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

    //Ascii,
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

pub fn read_object<R: io::Read>(r: &mut R) -> Result<Object, UnmarshalError> {
    let byte = read_byte!(r);
    let _flag = byte & 0b10000000; // TODO: do something with this
    let opcode = byte & 0b01111111;
    let object = match opcode as char {
        '0' => return Err(UnmarshalError::UnexpectedCode("NULL object in marshal data for object".to_string())),
        'N' => Object::None,
        'F' => Object::False,
        'T' => Object::True,
        'i' => {
            let mut buf = [0, 0, 0, 0];
            match r.read_exact(&mut buf) {
                Err(err) => return Err(UnmarshalError::Io(err)),
                Ok(()) => ()
            };
            Object::Int(buf[0] as u32 + 256*(buf[1] as u32 + 256*(buf[2] as u32 + 256*(buf[3] as u32))))
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
