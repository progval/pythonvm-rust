#[derive(PartialEq)]
#[derive(Debug)]
pub enum CmpOperator {
    Lt,  // Lower than
    Leq, // Lower or equal
    Eq,  // Equal
    Neq, // Not equal
    Gt,  // Greater than
    Geq, // Greater or equal
    In,
    NotIn,
    Is,
    IsNot,
    ExceptionMatch,
}

impl CmpOperator {
    pub fn from_bytecode(n: u16) -> Self {
        match n {
            0 => CmpOperator::Lt,
            1 => CmpOperator::Leq,
            2 => CmpOperator::Eq,
            3 => CmpOperator::Neq,
            4 => CmpOperator::Gt,
            5 => CmpOperator::Geq,
            6 => CmpOperator::In,
            7 => CmpOperator::NotIn,
            8 => CmpOperator::Is,
            9 => CmpOperator::IsNot,
            10=> CmpOperator::ExceptionMatch,
            _ => panic!("Invalid cmp operator code.")
        }
    }
}

#[derive(PartialEq)]
#[derive(Debug)]
pub enum Instruction {
    PopTop,
    Nop,
    BinarySubscr,
    LoadBuildClass,
    ReturnValue,
    StoreName(usize),
    LoadConst(usize),
    LoadName(usize),
    LoadAttr(usize),
    SetupLoop(usize),
    CompareOp(CmpOperator),
    JumpForward(usize),
    PopJumpIfFalse(usize),
    LoadFast(usize),
    LoadGlobal(usize),
    CallFunction(usize, usize), // nb_args, nb_kwargs
    MakeFunction(usize, usize, usize), // nb_default_args, nb_default_kwargs, nb_annot
}

#[derive(Debug)]
pub struct InstructionDecoder<I> where I: Iterator {
    bytestream: I,
    pending_nops: u8, // Number of NOPs to be inserted after this instruction to match CPython's addresses (instructions have different sizes)
}

impl<I> InstructionDecoder<I> where I: Iterator {
    pub fn new(bytes: I) -> InstructionDecoder<I> {
        InstructionDecoder { bytestream: bytes, pending_nops: 0, }
    }
}

impl<'a, I> InstructionDecoder<I> where I: Iterator<Item=&'a u8> {
    fn read_byte(&mut self) -> u8 {
        match self.bytestream.next() {
            Some(b) => {
                self.pending_nops += 1;
                *b
            },
            _ => panic!("End of stream in the middle of an instruction."),
        }
    }
    fn read_argument(&mut self) -> u16 {
        match (self.bytestream.next(), self.bytestream.next()) {
            (Some(b1), Some(b2)) => {
                self.pending_nops += 2;
                ((*b2 as u16) << 8) + (*b1 as u16)},
            _ => panic!("End of stream in the middle of an instruction."),
        }
    }
}

impl<'a, I> Iterator for InstructionDecoder<I> where I: Iterator<Item=&'a u8> {
    type Item = Instruction;

    fn next(&mut self) -> Option<Instruction> {
        if self.pending_nops != 0 {
            self.pending_nops -= 1;
            return Some(Instruction::Nop)
        };
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
                107 => Instruction::CompareOp(CmpOperator::from_bytecode(self.read_argument())),
                110 => Instruction::JumpForward(self.read_argument() as usize + 2), // +2, because JumpForward takes 3 bytes, and the relative address is computed from the next instruction.
                114 => Instruction::PopJumpIfFalse(self.read_argument() as usize),
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
