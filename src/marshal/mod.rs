pub mod common;
pub mod decode;

use std::io;
use super::objects::{Code, ObjectContent, ObjectRef, ObjectStore};
use self::common::Object as MarshalObject;
use self::common::Code as MarshalCode;

macro_rules! translate_vector {
    ( $e:expr, $map:ident, $store:ident ) => { $e.into_iter().map(|o| translate_object(o, $map, $store)).collect() }
}

macro_rules! translate_code_field {
    ( $code:ident, $expected:ident, $field:ident, $map:ident, $store:ident, $error:expr ) => {
        match translate_object_content($code.$field, $map, $store) {
            ObjectContent::$expected(v) => v,
            _ => panic!($error),
        }
    };
}

// TODO: more fields
fn translate_code(c: MarshalCode, translation_map: &Vec<ObjectRef>, store: &mut ObjectStore) -> Code {
    let code = translate_code_field!(c, Bytes, code, translation_map, store, "Code.code object must be bytes.");
    let consts = translate_code_field!(c, Tuple, consts, translation_map, store, "Code.consts object must be a tuple.");
    let name_objs = translate_code_field!(c, Tuple, names, translation_map, store, "Code.names object must be a tuple.");
    let names = name_objs.iter().map(|obj| {
        match store.deref(obj).content {
            ObjectContent::String(ref name) => name.clone(),
            _ => panic!("At least one object in Code.names is not a string."),
        }
    }).collect();
    Code { code: code, consts: consts, names: names }
}

fn translate_object_content(marshal_object: MarshalObject, translation_map: &Vec<ObjectRef>, store: &mut ObjectStore) -> ObjectContent {
    match marshal_object {
        MarshalObject::Hole => panic!("Remaining hole."),
        MarshalObject::None => ObjectContent::None,
        MarshalObject::False => ObjectContent::False,
        MarshalObject::True => ObjectContent::True,
        MarshalObject::Int(i) => ObjectContent::Int(i),
        MarshalObject::String(s) => ObjectContent::String(s),
        MarshalObject::Bytes(v) => ObjectContent::Bytes(v),
        MarshalObject::Tuple(v) => {
            let v = translate_vector!(v, translation_map, store);
            ObjectContent::Tuple(v)
        },
        MarshalObject::List(v) => {
            let v = translate_vector!(v, translation_map, store);
            ObjectContent::List(v)
        },
        MarshalObject::Set(v) => {
            let v = translate_vector!(v, translation_map, store);
            ObjectContent::Set(v)
        },
        MarshalObject::FrozenSet(v) => {
            let v = translate_vector!(v, translation_map, store);
            ObjectContent::FrozenSet(v)
        },
        MarshalObject::Code(c) => {
            ObjectContent::Code(translate_code(*c, translation_map, store))
        },
        MarshalObject::Ref(_) => panic!("For references, call translate_and_allocate_object.")
    }
}

fn translate_object(marshal_object: MarshalObject, translation_map: &Vec<ObjectRef>, store: &mut ObjectStore) -> ObjectRef {
    match marshal_object {
        MarshalObject::Ref(i) => translation_map.get(i as usize).unwrap().clone(), // TODO: overflow check
        _ => {
            let obj_content = translate_object_content(marshal_object, translation_map, store);
            store.allocate(obj_content)
        },
    }
}

fn translate_objects(marshal_object: MarshalObject, references: Vec<MarshalObject>, store: &mut ObjectStore) -> ObjectRef {
    let mut translation_map = Vec::new();
    translation_map.reserve(references.len());
    for obj in references {
        let obj = translate_object(obj, &translation_map, store);
        translation_map.push(obj);
    }
    translate_object(marshal_object, &translation_map, store)
}

pub fn read_object<R: io::Read>(reader: &mut R, store: &mut ObjectStore) -> Result<ObjectRef, decode::UnmarshalError> {
    let mut references = Vec::new();
    let marshal_object = try!(decode::read_tmp_object(reader, &mut references));
    Ok(translate_objects(marshal_object, references, store))
}
