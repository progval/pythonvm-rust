#[derive(PartialEq)]
#[derive(Debug)]
pub enum Instruction {
    PopTop,
    BinarySubscr,
    LoadBuildClass,
    ReturnValue,
    StoreName(usize),
    LoadConst(usize),
    LoadName(usize),
    LoadAttr(usize),
    SetupLoop(usize),
    LoadFast(usize),
    LoadGlobal(usize),
    CallFunction(usize, usize), // nb_args, nb_kwargs
    MakeFunction(usize, usize, usize), // nb_default_args, nb_default_kwargs, nb_annot
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
        match self.bytestream.next() {
            Some(b) => *b,
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
                25 => Instruction::BinarySubscr,
                71 => Instruction::LoadBuildClass,
                83 => Instruction::ReturnValue,
                90 => Instruction::StoreName(self.read_argument() as usize),
                100 => Instruction::LoadConst(self.read_argument() as usize),
                101 => Instruction::LoadName(self.read_argument() as usize),
                106 => Instruction::LoadAttr(self.read_argument() as usize),
                116 => Instruction::LoadGlobal(self.read_argument() as usize),
                120 => Instruction::SetupLoop(self.read_argument() as usize),
                124 => Instruction::LoadFast(self.read_argument() as usize),
                131 => Instruction::CallFunction(self.read_byte() as usize, self.read_byte() as usize),
                132 => {
                    let arg = self.read_argument();
                    let nb_pos = arg & 0xFF;
                    let nb_kw = (arg >> 8) & 0xFF;
                    //let nb_annot = (arg >> 16) & 0x7FF; // TODO
                    let nb_annot = 0;
                    Instruction::MakeFunction(nb_pos as usize, nb_kw as usize, nb_annot as usize)
                },
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
