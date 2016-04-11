pub trait Stack {
    type Item;

    fn top(&self) -> Option<&Self::Item>;
    fn pop(&mut self) -> Option<Self::Item>;
    fn push(&mut self, value: Self::Item);
}

pub struct VectorStack<Item: Sized> {
    vector: Vec<Item>
}

impl<Item> VectorStack<Item> {
    pub fn new() -> VectorStack<Item> {
        VectorStack { vector: Vec::new() }
    }
}

impl<Item> Stack for VectorStack<Item> {
    type Item = Item;

    fn top(&self) -> Option<&Self::Item> {
        self.vector.last()
    }

    fn pop(&mut self) -> Option<Self::Item> {
        self.vector.pop()
    }

    fn push(&mut self, value: Self::Item) {
        self.vector.push(value)
    }
}
