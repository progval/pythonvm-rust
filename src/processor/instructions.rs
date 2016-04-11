use std::str::Bytes;

#[derive(PartialEq)]
#[derive(Debug)]
pub enum Instruction {
    PopTop,
    ReturnValue,
    LoadConst(usize),
    LoadName(usize),
    LoadFast(u16),
    CallFunction(u16),
}

#[derive(Debug)]
pub struct InstructionDecoder<I> where I: Iterator {
    bytestream: I,
}

impl<I> InstructionDecoder<I> where I: Iterator {
    pub fn new(bytes: I) -> InstructionDecoder<I> {
        InstructionDecoder { bytestream: bytes }
    }
}

impl<'a, I> InstructionDecoder<I> where I: Iterator<Item=&'a u8> {
    fn read_byte(&mut self) -> u8 {
        match (self.bytestream.next(), self.bytestream.next()) {
            (Some(b1), Some(b2)) => {
                ((*b2 as u16) << 8) + (*b1 as u16)},
            _ => panic!("End of stream in the middle of an instruction."),
        }
    }
    fn read_argument(&mut self) -> u16 {
        match (self.bytestream.next(), self.bytestream.next()) {
            (Some(b1), Some(b2)) => {
                ((*b2 as u16) << 8) + (*b1 as u16)},
            _ => panic!("End of stream in the middle of an instruction."),
        }
    }
}

impl<'a, I> Iterator for InstructionDecoder<I> where I: Iterator<Item=&'a u8> {
    type Item = Instruction;

    fn next(&mut self) -> Option<Instruction> {
        self.bytestream.next().map(|opcode| {
            match *opcode {
                1 => Instruction::PopTop,
                83 => Instruction::ReturnValue,
                100 => Instruction::LoadConst(self.read_argument() as usize),
                101 => Instruction::LoadName(self.read_argument() as usize),
                124 => Instruction::LoadFast(self.read_argument()),
                131 => Instruction::CallFunction(self.read_byte(), self.read_byte()),
                _ => panic!(format!("Opcode not supported: {}", opcode)),
            }
        })
    }
}

#[test]
fn test_load_read() {
    let bytes: Vec<u8> = vec![124, 1, 0, 83];
    let reader = InstructionDecoder::new(bytes.iter());
    let instructions: Vec<Instruction> = reader.collect();
    assert_eq!(vec![Instruction::LoadFast(1), Instruction::ReturnValue], instructions);
}
