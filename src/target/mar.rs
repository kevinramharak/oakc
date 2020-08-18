use super::Target;
use std::{
    env::consts::EXE_SUFFIX,
    fs::{remove_file, write, read_to_string},
    fmt::{Debug, Display},
    io::{Error, ErrorKind, Result, Write},
    path::PathBuf,
    convert::TryFrom,
    collections::BTreeMap,
    process::exit,
};

use asciicolor::Colorize;
use crate::{parse,asm::AsmError};

// TODO: we want to compile to mir and then add that to the current compilation object
fn generate_stdlib(cwd: &PathBuf, input: String, target: &impl Target) -> Result<String> {
    let mut hir = parse(input);

    match hir.compile(cwd, target, &mut BTreeMap::new()) {
        Ok(mir) => match mir.assemble() {
            Ok(asm) => {
                // Set up the output code
                let mut result = String::new();
        
                // Iterate over the external files to include
                for filename in asm.get_externs() {
                    // Find them in the current working directory
                    if let Ok(contents) = read_to_string(filename.clone()) {
                        // Add the contents of the file to the result
                        result += &contents
                    } else {
                        // If the file doesn't exist, throw an error
                        if let Ok(name) = filename.clone().into_os_string().into_string() {
                            eprintln!("compilation error while generating 'std.mar': {}", format!("could not find foreign file '{}'", name).bright_red().underline());
                        } else {
                            eprintln!("compilation error while generating 'std.mar': {}", String::from("could not find foreign file").bright_red().underline());
                        }
                        exit(1);
                    }
                }

                return Ok(result);
            },
            Err(e) => {
                eprintln!("compilation error while generating 'std.mar': {}", e.bright_red().underline());
                exit(1);
            }
        }
        Err(e) => {
            eprintln!("compilation error while generating 'std.mar': {}", e.bright_red().underline());
            exit(1);
        }
    }
}

pub struct MAR;

// TODO: would like these to be members of the struct
// but that would require MAR to be mutable for the Target impl
// thus requirering the other target implementations to also update their function signatures to &mut self
static mut saved_global_scope_size: u16 = 0;
static mut saved_init_vm_capacity: u16 = 0;
static mut unique_id: i32 = 0;
static mut loop_identifiers: Vec<i32> = Vec::new(); 


impl Target for MAR {
    fn get_name(&self) -> char {
        'm'
    }

    fn std(&self) -> String {
        // we use the oak compiler itself to compile our stdlib
        // we kinda have to because MAR has no import system or compile time checks available
        // pathbuff from project root working directory?
        if let Ok(std) = generate_stdlib(&PathBuf::from("src/target/std/mar/"), String::from(include_str!("std/std.mar.ok")), self) {
            return std;
        }
        String::from(";; ERROR: could not generate the 'std.mar' file")
    }

    fn core_prelude(&self) -> String {
        String::from(include_str!("core/core.mar"))
    }

    fn core_postlude(&self) -> String {
        String::from("__core_heap_start: ;; heap starts at this address")
    }

    fn begin_entry_point(&self, global_scope_size: i32, memory_size: i32) -> String {
        // because of how constants need to be manually hoisted in MAR assembly
        // we save these values and prefix them to the code in the compile function
        unsafe {
            saved_global_scope_size = u16::try_from(global_scope_size).ok().unwrap();
            saved_init_vm_capacity = u16::try_from(memory_size).ok().unwrap();
        }
        String::from(format!(r##"
;; start of entry point
__core_main:
"##))
    }

    fn end_entry_point(&self) -> String {
        // technically we want to get the return value from main and return it to the hosting environment
        // but since the target implementation is the host, we can do whatever we want here
        // TODO: remove the call to __mar_comport_flush from core
        String::from("    call __mar_comport_flush\n    RET ;; return from entry point\n")
    }

    fn establish_stack_frame(&self, arg_size: i32, local_scope_size: i32) -> String {
        String::from(format!(
r#"    push {} ;; local_scope_size
    push {} ;; arg_size
    call __core_machine_establish_stack_frame
"#, u16::try_from(local_scope_size).ok().unwrap(), u16::try_from(arg_size).ok().unwrap()))
    }

    fn end_stack_frame(&self, return_size: i32, local_scope_size: i32) -> String {
        String::from(format!(
r#"    push {} ;; local_scope_size
    push {} ;; return size
    call __core_machine_end_stack_frame
"#, u16::try_from(local_scope_size).ok().unwrap(), u16::try_from(return_size).ok().unwrap()))
    }

    fn load_base_ptr(&self) -> String {
        String::from("    call __core_machine_load_base_ptr ;; push the base pointer on the stack\n")
    }

    fn push(&self, n: f64) -> String {
        // TODO: i16::try_from><f64>() is not implemented? kinda want to do a checked cast here
        String::from(format!(
r##"    push {} ;; push value on the vm stack
    call __core_machine_push
"##, n as i16))
    }

    fn add(&self) -> String {
        String::from("    call __core_machine_add\n")
    }

    fn subtract(&self) -> String {
        String::from("    call __core_machine_subtract\n")
    }
    
    fn multiply(&self) -> String {
        String::from("    call __core_machine_multiply\n")
    }
    
    fn divide(&self) -> String {
        String::from("    call __core_machine_divide\n")
    }

    fn sign(&self) -> String {
        String::from("    call __core_machine_sign\n")
    }

    fn allocate(&self) -> String {
        String::from("    call __core_machine_allocate\n")
    }

    fn free(&self) -> String {
        String::from("    call __core_machine_free\n")
    }

    fn store(&self, size: i32) -> String {
        // TODO: i16::try_from><f64>() is not implemented? kinda want to do a checked cast here
        String::from(format!(
r##"    push {} ;; size
    call __core_machine_store
"##, size as i16))
    }

    fn load(&self, size: i32) -> String {
        // TODO: i16::try_from><f64>() is not implemented? kinda want to do a checked cast here
        String::from(format!(
r##"    push {} ;; size
    call __core_machine_load
"##, size as i16))
    }

    fn fn_header(&self, name: String) -> String {
        String::new()
    }

    fn fn_definition(&self, name: String, body: String) -> String {
        String::from(format!(r##"
{}:       ;; definition of {}
{}    ret ;; returning from {}
"##, name, name, body, name))
    }

    fn call_fn(&self, name: String) -> String {
        String::from(format!("    call {} ;; calling oak function\n", name))
    }

    fn call_foreign_fn(&self, name: String) -> String {
        String::from(format!("    call {} ;; calling foreign function\n", name))
    }

    fn begin_while(&self) -> String {
        unsafe {
            let id = unique_id;
            loop_identifiers.push(id);
            unique_id += 1;
            let str = String::from(format!(
r#"__generated_begin_while_{}:
    call __core_machine_pop
    cmp A, 0
    jz __generated_end_while_{}
"#, id, id));
            str
        }
    }

    fn end_while(&self) -> String {
        unsafe {
            let id = loop_identifiers.pop().unwrap();
            let str = String::from(format!(
r#"    jmp __generated_begin_while_{}
__generated_end_while_{}:
"#, id, id));
            str
        }
    }

    fn compile(&self, code: String) -> Result<()> {
        // prefix the saved values as constants
        let code_with_prefixed_constants = String::from(format!(
r#"
__CORE_GLOBAL_SCOPE_SIZE equ {}
__CORE_INIT_VM_CAPACITY equ {}
"#, unsafe { saved_global_scope_size }, unsafe { saved_init_vm_capacity })) + code.as_str();
        if let Ok(_) = write("OUTPUT.mar", code_with_prefixed_constants) {
            return Result::Ok(())
        }
        return Result::Err(Error::new(ErrorKind::Other,
            "unabe to compile to MAR"));
    }
}
