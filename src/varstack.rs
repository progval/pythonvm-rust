use std::fmt::Debug;

pub trait VarStack : Debug {
    type Item;

    fn top(&self) -> Option<&Self::Item>;
    fn pop(&mut self) -> Option<Self::Item>;
    fn pop_many(&mut self, count: usize) -> Option<Vec<Self::Item>>;
    fn push(&mut self, value: Self::Item);
    fn pop_all_and_get_n_last(&mut self, nb: usize) -> Option<Vec<Self::Item>>;
}

#[derive(Debug)]
pub struct VectorVarStack<Item: Sized> {
    vector: Vec<Item>
}

impl<Item> VectorVarStack<Item> {
    pub fn new() -> VectorVarStack<Item> {
        VectorVarStack { vector: Vec::new() }
    }
}

impl<Item> VectorVarStack<Item> {
    pub fn iter(&self) -> ::std::slice::Iter<Item> {
        self.vector.iter()
    }
}

impl<Item> VarStack for VectorVarStack<Item> where Item: Debug {
    type Item = Item;

    fn top(&self) -> Option<&Self::Item> {
        self.vector.last()
    }

    fn pop(&mut self) -> Option<Self::Item> {
        self.vector.pop()
    }

    fn pop_many(&mut self, count: usize) -> Option<Vec<Self::Item>> {
        if count > self.vector.len() {
            None
        }
        else {
            let length = self.vector.len();
            Some(self.vector.drain((length-count)..length).into_iter().collect())
        }
    }

    fn push(&mut self, value: Self::Item) {
        self.vector.push(value)
    }

    fn pop_all_and_get_n_last(&mut self, nb: usize) -> Option<Vec<Self::Item>> {
        if self.vector.len() < nb {
            None
        }
        else {
            self.vector.truncate(nb);
            Some(self.vector.drain(..).collect())
        }
    }
}
