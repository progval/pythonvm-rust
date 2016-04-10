
#[derive(PartialEq)]
#[derive(Debug)]
pub struct Code {
    pub argcount: u32,
    pub kwonlyargcount: u32,
    pub nlocals: u32,
    pub stacksize: u32,
    pub flags: u32,
    pub code: Object,
    pub consts: Object,
    pub names: Object,
    pub varnames: Object,
    pub freevars: Object,
    pub cellvars: Object,
    pub filename: Object,
    pub name: Object,
    pub firstlineno: u32,
    pub lnotab: Object,
}

#[derive(PartialEq)]
#[derive(Debug)]
pub enum Object {
    Hole, // Temporary object for unmarshalling
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
    Tuple(Vec<Object>),
    List(Vec<Object>),
    //Dict,
    Code(Box<Code>),
    //Unknown,
    Set(Vec<Object>),
    FrozenSet(Vec<Object>),
    Ref(u32),

    Bytes(Vec<u8>), // aka. ASCII in CPython's marshal
    //ShortAscii,
    //ShortAsciiInterned
}
