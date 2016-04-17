pub mod decode;

use std::io;
use super::objects::{ObjectRef, ObjectStore, PrimitiveObjects};


pub fn read_object<R: io::Read>(reader: &mut R, store: &mut ObjectStore, primitive_objects: &PrimitiveObjects) -> Result<ObjectRef, decode::UnmarshalError> {
    decode::read_object(reader, store, primitive_objects, &mut Vec::new())
}
