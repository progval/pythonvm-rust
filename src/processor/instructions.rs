use super::super::objects::ObjectRef;

#[derive(PartialEq)]
#[derive(Debug)]
#[derive(Clone)]
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
    pub fn from_bytecode(n: u8) -> Self {
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
#[derive(Clone)]
pub enum Instruction {
    PushImmediate(ObjectRef),

    PopTop,
    DupTop,
    Nop,
    BinarySubscr,
    GetIter,
    LoadBuildClass,
    ReturnValue,
    PopBlock,
    EndFinally,
    PopExcept,
    StoreName(usize),
    ForIter(usize),
    StoreAttr(usize),
    StoreGlobal(usize),
    LoadConst(usize),
    LoadName(usize),
    BuildTuple(usize),
    LoadAttr(usize),
    SetupLoop(usize),
    SetupExcept(usize),
    CompareOp(CmpOperator),
    JumpForward(usize),
    JumpAbsolute(usize),
    PopJumpIfFalse(usize),
    LoadFast(usize),
    StoreFast(usize),
    LoadGlobal(usize),
    CallFunction(usize, usize), // nb_args, nb_kwargs
    RaiseVarargs(u16),
    MakeFunction { has_defaults: bool, has_kwdefaults: bool, has_annotations: bool, has_closure: bool },
    BuildConstKeyMap(usize),
}

#[derive(Debug)]
pub struct InstructionDecoder<I> where I: Iterator {
    bytestream: I,
    arg_prefix: Option<u32>,
    pending_nops: u8, // Number of NOPs to be inserted after this instruction to match CPython's addresses (instructions have different sizes)
}

impl<I> InstructionDecoder<I> where I: Iterator {
    pub fn new(bytes: I) -> InstructionDecoder<I> {
        InstructionDecoder { bytestream: bytes, pending_nops: 0, arg_prefix: None, }
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
    fn read_argument(&mut self) -> u32 {
        match (self.bytestream.next(), self.bytestream.next()) {
            (Some(b1), Some(b2)) => {
                self.pending_nops += 2;
                let arg = ((*b2 as u32) << 8) + (*b1 as u32);
                if let Some(prefix) = self.arg_prefix {
                    self.arg_prefix = None;
                    (prefix << 16) + arg
                }
                else {
                    arg
                }
            },
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
        let opcode = self.bytestream.next();
        let oparg = self.bytestream.next();
        if let (Some(opcode), Some(oparg)) = (opcode, oparg) {
            let opcode = *opcode;
            let oparg = *oparg;
            let inst = match opcode {
                1 => Instruction::PopTop,
                4 => Instruction::DupTop,
                25 => Instruction::BinarySubscr,
                68 => Instruction::GetIter,
                71 => Instruction::LoadBuildClass,
                83 => Instruction::ReturnValue,
                87 => Instruction::PopBlock,
                88 => Instruction::EndFinally,
                89 => Instruction::PopExcept,
                90 => Instruction::StoreName(oparg as usize),
                93 => Instruction::ForIter(oparg as usize),
                95 => Instruction::StoreAttr(oparg as usize),
                97 => Instruction::StoreGlobal(oparg as usize),
                100 => Instruction::LoadConst(oparg as usize),
                101 => Instruction::LoadName(oparg as usize),
                102 => Instruction::BuildTuple(oparg as usize),
                106 => Instruction::LoadAttr(oparg as usize),
                107 => Instruction::CompareOp(CmpOperator::from_bytecode(oparg)),
                110 => Instruction::JumpForward(oparg as usize),
                113 => Instruction::JumpAbsolute(oparg as usize),
                114 => Instruction::PopJumpIfFalse(oparg as usize),
                116 => Instruction::LoadGlobal(oparg as usize),
                120 => Instruction::SetupLoop(oparg as usize + 1),
                121 => Instruction::SetupExcept(oparg as usize + 1),
                124 => Instruction::LoadFast(oparg as usize),
                125 => Instruction::StoreFast(oparg as usize),
                130 => Instruction::RaiseVarargs(self.read_argument() as u16),
                131 => Instruction::CallFunction(oparg as usize, 0),
                132 => Instruction::MakeFunction {
                    has_defaults: oparg & 0x01 != 0,
                    has_kwdefaults: oparg & 0x02 != 0,
                    has_annotations: oparg & 0x04 != 0,
                    has_closure: oparg & 0x08 != 0,
                },
                156 => Instruction::BuildConstKeyMap(oparg as usize),
                144 => { self.arg_prefix = Some(self.read_argument()); Instruction::Nop },
                _ => panic!(format!("Opcode not supported: {:?}", (opcode, oparg))),
            };
            Some(inst)
        }
        else {
            None
        }
    }
}

#[test]
fn test_load_read() {
    let bytes: Vec<u8> = vec![124, 1, 0, 83];
    let reader = InstructionDecoder::new(bytes.iter());
    let instructions: Vec<Instruction> = reader.collect();
    assert_eq!(vec![Instruction::LoadFast(1), Instruction::Nop, Instruction::Nop, Instruction::ReturnValue], instructions);
}
