pub mod instructions;

use super::objects::{Code, ObjectStore, ObjectRef, ObjectContent};
use super::sandbox::EnvProxy;
use super::stack::{Stack, VectorStack};
use self::instructions::Instruction;
use std::fmt;
use std::collections::HashMap;
use std::io::Write;
use std::io::Read;
use super::marshal;

#[derive(Debug)]
pub enum ProcessorError {
    CircularReference,
    InvalidReference,
    NotACodeObject(String),
    CodeObjectIsNotBytes,
    InvalidProgramCounter,
    StackTooSmall,
    InvalidConstIndex,
    InvalidNameIndex,
    InvalidVarnameIndex,
    UnknownPrimitive(String),
    UnmarshalError(marshal::decode::UnmarshalError),
    Exception(String),
}

impl fmt::Display for ProcessorError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
       fmt::Debug::fmt(self, f)
    }
}


pub type PyFunction<EP> = fn(&mut Processor<EP>, Vec<ObjectRef>) -> Result<ObjectRef, ProcessorError>;

pub struct Processor<EP: EnvProxy> {
    pub envproxy: EP,
    pub store: ObjectStore,
    pub primitives: HashMap<String, PyFunction<EP>>,
}

impl<EP: EnvProxy> Processor<EP> {

    fn load_name(&mut self, namespace: &mut HashMap<String, ObjectRef>, name: String) -> Result<ObjectRef, ProcessorError> {
        if name == "__primitives__" {
            Ok(self.store.allocate(ObjectContent::PrimitiveNamespace))
        }
        else if let Some(obj_ref) = namespace.get(&name) {
            Ok(obj_ref.clone())
        }
        else {
            panic!(format!("Cannot load {}: neither a primitive or in namespace.", name))
        }
    }

    fn call_function(&mut self, namespace: &mut HashMap<String, ObjectRef>, func_ref: &ObjectRef, args: Vec<ObjectRef>, kwargs: Vec<ObjectRef>) -> Result<ObjectRef, ProcessorError> {
        // TODO: clone only if necessary
        match self.store.deref(func_ref).content.clone() {
            ObjectContent::Code(code) => {
                let mut namespace = namespace.clone(); // TODO: costly, try maybe copy-on-write?
                if code.argcount != args.len() {
                    panic!(format!("Function {} expected {} arguments, but got {}.", code.name, code.argcount, args.len()))
                };
                for (argname, argvalue) in code.varnames.iter().zip(args) {
                    namespace.insert(argname.clone(), argvalue);
                };
                self.run_code(&mut namespace, (*code).clone())
            },
            ObjectContent::Function(ref _name, ref code_ref) => {
                self.call_function(namespace, code_ref, args, kwargs)
            },
            ObjectContent::PrimitiveFunction(ref name) => {
                let function = match self.primitives.get(name) {
                    Some(function) => function.clone(),
                    None => return Err(ProcessorError::UnknownPrimitive(name.clone())),
                };
                function(self, args)
            },
            ref o => return Err(ProcessorError::NotACodeObject(format!("{:?}", o))),
        }
    }

    fn run_code(&mut self, namespace: &mut HashMap<String, ObjectRef>, code: Code) -> Result<ObjectRef, ProcessorError> {
        let bytecode: Vec<u8> = code.code;
        let instructions: Vec<Instruction> = instructions::InstructionDecoder::new(bytecode.iter()).into_iter().collect();
        let mut program_counter = 0 as usize;
        let mut stack = VectorStack::new();
        loop {
            let instruction = try!(instructions.get(program_counter).ok_or(ProcessorError::InvalidProgramCounter));
            program_counter += 1;
            match *instruction {
                Instruction::PopTop => {
                    try!(stack.pop().ok_or(ProcessorError::StackTooSmall));
                    ()
                },
                Instruction::ReturnValue => return Ok(try!(stack.pop().ok_or(ProcessorError::StackTooSmall))),
                Instruction::StoreName(i) => {
                    let name = try!(code.names.get(i).ok_or(ProcessorError::InvalidNameIndex)).clone();
                    let obj = try!(stack.pop().ok_or(ProcessorError::StackTooSmall));
                    namespace.insert(name, obj);
                }
                Instruction::LoadConst(i) => stack.push(try!(code.consts.get(i).ok_or(ProcessorError::InvalidConstIndex)).clone()),
                Instruction::LoadName(i) | Instruction::LoadGlobal(i) => {
                    let name = try!(code.names.get(i).ok_or(ProcessorError::InvalidNameIndex)).clone();
                    stack.push(try!(self.load_name(namespace, name)))
                }
                Instruction::LoadFast(i) => {
                    let name = try!(code.varnames.get(i).ok_or(ProcessorError::InvalidVarnameIndex)).clone();
                    stack.push(try!(self.load_name(namespace, name)))
                }
                Instruction::LoadAttr(i) => {
                    let obj_ref = try!(stack.top().ok_or(ProcessorError::StackTooSmall)).clone();
                    let name = try!(code.names.get(i).ok_or(ProcessorError::InvalidNameIndex)).clone();
                    if let ObjectContent::PrimitiveNamespace = self.store.deref(&obj_ref).content {
                        stack.push(self.store.allocate(ObjectContent::PrimitiveFunction(name)))
                    }
                    else {
                        unimplemented!();
                    }
                },
                Instruction::CallFunction(nb_args, nb_kwargs) => {
                    // See “Call constructs” at:
                    // http://security.coverity.com/blog/2014/Nov/understanding-python-bytecode.html
                    let kwargs = try!(stack.pop_many(nb_kwargs*2).ok_or(ProcessorError::StackTooSmall));
                    let args = try!(stack.pop_many(nb_args).ok_or(ProcessorError::StackTooSmall));
                    let func = try!(stack.pop().ok_or(ProcessorError::StackTooSmall));
                    let ret_value = self.call_function(namespace, &func, args, kwargs);
                    stack.push(try!(ret_value))
                },
                Instruction::MakeFunction(0, 0, 0) => {
                    // TODO: consume default arguments and annotations
                    let func_name = match self.store.deref(&try!(stack.pop().ok_or(ProcessorError::StackTooSmall))).content {
                        ObjectContent::String(ref s) => s.clone(),
                        _ => panic!("Function names must be strings."),
                    };
                    let code = try!(stack.pop().ok_or(ProcessorError::StackTooSmall));
                    stack.push(self.store.allocate(ObjectContent::Function(func_name, code)))
                },
                _ => panic!(format!("todo: instruction {:?}", *instruction)),
            }
        };
    }

    pub fn run_module(&mut self, namespace: &mut HashMap<String, ObjectRef>, module_name: String) -> Result<(), ProcessorError> {
        let mut builtins_bytecode = self.envproxy.open_module(module_name);
        let mut buf = [0; 12];
        builtins_bytecode.read_exact(&mut buf);
        let builtins_code = try!(marshal::read_object(&mut builtins_bytecode, &mut self.store).map_err(ProcessorError::UnmarshalError));
        let builtins_code = match self.store.deref(&builtins_code).content {
            ObjectContent::Code(ref code) => code.clone(),
            ref o => return Err(ProcessorError::NotACodeObject(format!("{:?}", o))),
        };
        try!(self.run_code(namespace, *builtins_code));
        Ok(())
    }

    pub fn run_code_object(&mut self, module: ObjectRef) -> Result<ObjectRef, ProcessorError> {
        let mut builtins = HashMap::new();
        self.run_module(&mut builtins, "builtins".to_string());

        let code = match self.store.deref(&module).content {
            ObjectContent::Code(ref code) => code.clone(),
            ref o => return Err(ProcessorError::NotACodeObject(format!("{:?}", o))),
        };
        self.run_code(&mut builtins, *code)
    }
}
