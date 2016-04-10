pub mod common;
pub mod decode;

use std::io;
use std::collections::HashSet;
use super::objects::{Code, ObjectContent, Object, ObjectRef, ObjectStore};
use self::common::Object as MarshalObject;

macro_rules! translate_vector {
    ( $e:expr, $map:ident, $store:ident ) => { $e.into_iter().map(|o| translate_object(o, $map, $store)).collect() }
}

fn translate_object(marshal_object: MarshalObject, translation_map: &Vec<ObjectRef>, store: &mut ObjectStore) -> ObjectRef {
    match marshal_object {
        MarshalObject::Hole => panic!("Remaining hole."),
        MarshalObject::None => store.allocate(ObjectContent::None),
        MarshalObject::False => store.allocate(ObjectContent::False),
        MarshalObject::True => store.allocate(ObjectContent::True),
        MarshalObject::Int(i) => store.allocate(ObjectContent::Int(i)),
        MarshalObject::String(s) => store.allocate(ObjectContent::String(s)),
        MarshalObject::Bytes(v) => store.allocate(ObjectContent::Bytes(v)),
        MarshalObject::Tuple(v) => {
            let v = translate_vector!(v, translation_map, store);
            store.allocate(ObjectContent::Tuple(v))
        },
        MarshalObject::List(v) => {
            let v = translate_vector!(v, translation_map, store);
            store.allocate(ObjectContent::List(v))
        },
        MarshalObject::Set(v) => {
            let v = translate_vector!(v, translation_map, store);
            store.allocate(ObjectContent::Set(v))
        },
        MarshalObject::FrozenSet(v) => {
            let v = translate_vector!(v, translation_map, store);
            store.allocate(ObjectContent::FrozenSet(v))
        },
        MarshalObject::Code(c) => {
            let code = translate_object(c.code, translation_map, store);
            store.allocate(ObjectContent::Code(Code { code: code})) // TODO: more fields
        },
        MarshalObject::Ref(i) => translation_map.get(i as usize).unwrap().clone(), // TODO: overflow check
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
