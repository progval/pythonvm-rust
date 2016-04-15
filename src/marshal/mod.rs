pub mod decode;

use std::io;
use super::objects::{ObjectRef, ObjectStore};


pub fn read_object<R: io::Read>(reader: &mut R, store: &mut ObjectStore) -> Result<ObjectRef, decode::UnmarshalError> {
    decode::read_object(reader, store, &mut Vec::new())
}
