pub mod instructions;

use super::objects::{Code, ObjectStore, ObjectRef, ObjectContent, PrimitiveObjects, Object};
use super::sandbox::EnvProxy;
use super::stack::{Stack, VectorStack};
use self::instructions::Instruction;
use std::fmt;
use std::collections::HashMap;
use std::io::Read;
use super::marshal;

#[derive(Debug)]
pub enum ProcessorError {
    CircularReference,
    InvalidReference,
    NotACodeObject(String),
    NotAFunctionObject(String),
    CodeObjectIsNotBytes,
    InvalidProgramCounter,
    StackTooSmall,
    InvalidConstIndex,
    InvalidNameIndex,
    InvalidVarnameIndex,
    UnknownPrimitive(String),
    UnmarshalError(marshal::decode::UnmarshalError),
}

impl fmt::Display for ProcessorError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
       fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug)]
pub enum PyResult {
    Return(ObjectRef),
    Raise(ObjectRef),
}


pub type PyFunction<EP> = fn(&mut Processor<EP>, Vec<ObjectRef>) -> Result<PyResult, ProcessorError>;

#[derive(Debug)]
struct Loop {
    begin: usize,
    end: usize,
}

struct Stacks {
    var_stack: VectorStack<ObjectRef>,
    loop_stack: VectorStack<Loop>,
}

macro_rules! pop_stack {
    ( $stack_name:expr) => {
        try!($stack_name.pop().ok_or(ProcessorError::StackTooSmall))
    }
}

pub struct Processor<EP: EnvProxy> {
    pub envproxy: EP,
    pub store: ObjectStore,
    pub primitive_functions: HashMap<String, PyFunction<EP>>,
    pub primitive_objects: PrimitiveObjects
}

impl<EP: EnvProxy> Processor<EP> {

    // Load a name from the namespace (only __primitive__ and locals for now)
    fn load_name(&mut self, namespace: &mut HashMap<String, ObjectRef>, name: String) -> Result<ObjectRef, ProcessorError> {
        if name == "__primitives__" {
            Ok(self.store.allocate(Object { name: Some("__primitives__".to_string()), content: ObjectContent::PrimitiveNamespace, class: self.primitive_objects.object.clone(), bases: None }))
        }
        else if name == "__name__" {
            Ok(self.store.allocate(self.primitive_objects.new_string("<module>".to_string())))
        }
        else if let Some(obj_ref) = namespace.get(&name) {
            Ok(obj_ref.clone())
        }
        else {
            panic!(format!("Cannot load {}: neither __primitives__ or in namespace.", name))
        }
    }


    fn load_attr(&mut self, obj: &Object, name: String) -> Result<ObjectRef, ProcessorError> {
        match name.as_ref() {
            "__bases__" => {
                match obj.bases {
                    Some(ref v) => Ok(self.store.allocate(self.primitive_objects.new_tuple(v.clone()))),
                    None => Ok(self.primitive_objects.none.clone()),
                }
            },
            "__name__" => {
                match obj.name {
                    Some(ref s) => Ok(self.store.allocate(self.primitive_objects.new_string(s.clone()))),
                    None => panic!("No __name__"),
                }
            },
            _ => {
                if let ObjectContent::PrimitiveNamespace = obj.content {
                    match self.primitive_objects.names_map.get(&name) {
                        Some(obj_ref) => Ok(obj_ref.clone()),
                        None => Ok(self.store.allocate(Object { name: Some(name.clone()), content: ObjectContent::PrimitiveFunction(name), class: self.primitive_objects.function_type.clone(), bases: None })),
                    }
                }
                else {
                    panic!(format!("Not implemented: looking up attribute '{}' of {:?}", name, obj))
                }
            }
        }
    }

    // Call a primitive / function / code object, with arguments.
    fn call_function(&mut self, namespace: &mut HashMap<String, ObjectRef>, func_ref: &ObjectRef, mut args: Vec<ObjectRef>, kwargs: Vec<ObjectRef>) -> Result<PyResult, ProcessorError> {
        // TODO: clone only if necessary
        match self.store.deref(func_ref).content.clone() {
            ObjectContent::Class(None) => {
                Ok(PyResult::Return(self.store.allocate(Object::new_instance(None, func_ref.clone(), ObjectContent::OtherObject))))
            },
            ObjectContent::Class(Some(ref code_ref)) => {
                // TODO: run code
                Ok(PyResult::Return(self.store.allocate(Object::new_instance(None, func_ref.clone(), ObjectContent::OtherObject))))
            },
            ObjectContent::Function(ref code_ref) => {
                match self.store.deref(code_ref).content.clone() {
                    ObjectContent::Code(code) => {
                        let mut namespace = namespace.clone(); // TODO: costly, try maybe copy-on-write?
                        if code.co_varargs() { // If it has a *args argument
                            if code.argcount > args.len() {
                                panic!(format!("Function {} expected at least {} arguments, but got {}.", code.name, code.argcount, args.len()))
                            };
                            let to_vararg = args.drain(code.argcount..).collect();
                            let obj_ref = self.store.allocate(self.primitive_objects.new_tuple(to_vararg));
                            args.push(obj_ref);
                        }
                        else if code.argcount != args.len() {
                            panic!(format!("Function {} expected {} arguments, but got {}.", code.name, code.argcount, args.len()))
                        };
                        for (argname, argvalue) in code.varnames.iter().zip(args) {
                            namespace.insert(argname.clone(), argvalue);
                        };
                        self.run_code(&mut namespace, (*code).clone())
                    },
                    ref o => {
                        match self.store.deref(func_ref).name {
                            None => return Err(ProcessorError::NotACodeObject(format!("anonymous function has code {:?}", o))),
                            Some(ref name) => return Err(ProcessorError::NotACodeObject(format!("function {} has code {:?}", name, o))),
                        }
                    }
                }
            },
            ObjectContent::PrimitiveFunction(ref name) => {
                let function = match self.primitive_functions.get(name) {
                    Some(function) => function.clone(),
                    None => return Err(ProcessorError::UnknownPrimitive(name.clone())),
                };
                function(self, args)
            },
            _ => {
                return Err(ProcessorError::NotAFunctionObject(format!("called {:?}", self.store.deref(func_ref))));
            }
        }
    }

    // Main interpreter loop
    // See https://docs.python.org/3/library/dis.html for a description of instructions
    fn run_code(&mut self, namespace: &mut HashMap<String, ObjectRef>, code: Code) -> Result<PyResult, ProcessorError> {
        let bytecode: Vec<u8> = code.code;
        let instructions: Vec<Instruction> = instructions::InstructionDecoder::new(bytecode.iter()).into_iter().collect();
        let mut program_counter = 0 as usize;
        let mut stacks = Stacks { var_stack: VectorStack::new(), loop_stack: VectorStack::new() };
        loop {
            let instruction = try!(instructions.get(program_counter).ok_or(ProcessorError::InvalidProgramCounter));
            match *instruction {
                Instruction::PopTop => {
                    pop_stack!(stacks.var_stack);
                    ()
                },
                Instruction::BinarySubscr => {
                    let index_ref = pop_stack!(stacks.var_stack);
                    let index = self.store.deref(&index_ref).content.clone();
                    let container_ref = pop_stack!(stacks.var_stack);
                    let container = self.store.deref(&container_ref).content.clone();
                    match (container, index) {
                        (ObjectContent::Tuple(v), ObjectContent::Int(i)) | (ObjectContent::List(v), ObjectContent::Int(i)) => {
                            match v.get(i as usize) { // TODO: overflow check
                                None => panic!("Out of bound"),
                                Some(obj_ref) => stacks.var_stack.push(obj_ref.clone()),
                            }
                        }
                        _ => panic!("Indexing only supported for tuples/lists with an integer.")
                    }
                }
                Instruction::LoadBuildClass => {
                    let obj = Object { name: Some("__build_class__".to_string()), content: ObjectContent::PrimitiveFunction("build_class".to_string()), class: self.primitive_objects.function_type.clone(), bases: None };
                    stacks.var_stack.push(self.store.allocate(obj));
                }
                Instruction::ReturnValue => return Ok(PyResult::Return(pop_stack!(stacks.var_stack))),
                Instruction::StoreName(i) => {
                    let name = try!(code.names.get(i).ok_or(ProcessorError::InvalidNameIndex)).clone();
                    let obj_ref = pop_stack!(stacks.var_stack);
                    namespace.insert(name, obj_ref);
                }
                Instruction::LoadConst(i) => stacks.var_stack.push(try!(code.consts.get(i).ok_or(ProcessorError::InvalidConstIndex)).clone()),
                Instruction::LoadName(i) | Instruction::LoadGlobal(i) => {
                    let name = try!(code.names.get(i).ok_or(ProcessorError::InvalidNameIndex)).clone();
                    stacks.var_stack.push(try!(self.load_name(namespace, name)))
                }
                Instruction::SetupLoop(i) => {
                    stacks.loop_stack.push(Loop { begin: program_counter, end: program_counter+i })
                }
                Instruction::LoadFast(i) => {
                    let name = try!(code.varnames.get(i).ok_or(ProcessorError::InvalidVarnameIndex)).clone();
                    stacks.var_stack.push(try!(self.load_name(namespace, name)))
                }
                Instruction::LoadAttr(i) => {
                    let obj = {
                        let obj_ref = try!(stacks.var_stack.pop().ok_or(ProcessorError::StackTooSmall));
                        self.store.deref(&obj_ref).clone()
                    };
                    let name = try!(code.names.get(i).ok_or(ProcessorError::InvalidNameIndex)).clone();
                    stacks.var_stack.push(try!(self.load_attr(&obj, name)))
                },
                Instruction::CallFunction(nb_args, nb_kwargs) => {
                    // See “Call constructs” at:
                    // http://security.coverity.com/blog/2014/Nov/understanding-python-bytecode.html
                    let kwargs = try!(stacks.var_stack.pop_many(nb_kwargs*2).ok_or(ProcessorError::StackTooSmall));
                    let args = try!(stacks.var_stack.pop_many(nb_args).ok_or(ProcessorError::StackTooSmall));
                    let func = pop_stack!(stacks.var_stack);
                    let ret = try!(self.call_function(namespace, &func, args, kwargs));
                    match ret {
                        PyResult::Return(obj_ref) => stacks.var_stack.push(obj_ref),
                        PyResult::Raise(obj_ref) => return Ok(PyResult::Raise(obj_ref))
                    };
                },
                Instruction::MakeFunction(0, 0, 0) => {
                    // TODO: consume default arguments and annotations
                    let func_name = match self.store.deref(&pop_stack!(stacks.var_stack)).content {
                        ObjectContent::String(ref s) => s.clone(),
                        _ => panic!("Function names must be strings."),
                    };
                    let code = pop_stack!(stacks.var_stack);
                    stacks.var_stack.push(self.store.allocate(Object { name: Some(func_name), content: ObjectContent::Function(code), class: self.primitive_objects.function_type.clone(), bases: None }))
                },
                _ => panic!(format!("todo: instruction {:?}", *instruction)),
            }
            program_counter += 1;
        };
    }

    /// Load a module from its name and run it.
    /// Functions and attributes will be added in the `namespace`.
    pub fn run_module(&mut self, namespace: &mut HashMap<String, ObjectRef>, module_name: String) -> Result<PyResult, ProcessorError> {
        let mut builtins_bytecode = self.envproxy.open_module(module_name);
        let mut buf = [0; 12];
        builtins_bytecode.read_exact(&mut buf).unwrap();
        if !marshal::check_magic(&buf[0..4]) {
            panic!("Bad magic number for builtins.py.")
        }
        let builtins_code = try!(marshal::read_object(&mut builtins_bytecode, &mut self.store, &self.primitive_objects).map_err(ProcessorError::UnmarshalError));
        let builtins_code = match self.store.deref(&builtins_code).content {
            ObjectContent::Code(ref code) => code.clone(),
            ref o => return Err(ProcessorError::NotACodeObject(format!("builtins code {:?}", o))),
        };
        self.run_code(namespace, *builtins_code)
    }

    /// Entry point to run code. Loads builtins in the code's namespace and then run it.
    pub fn run_code_object(&mut self, code_object: ObjectRef) -> Result<PyResult, ProcessorError> {
        let mut builtins = HashMap::new();
        try!(self.run_module(&mut builtins, "builtins".to_string()));

        let code = match self.store.deref(&code_object).content {
            ObjectContent::Code(ref code) => code.clone(),
            ref o => return Err(ProcessorError::NotACodeObject(format!("file code {:?}", o))),
        };
        self.run_code(&mut builtins, *code)
    }
}
