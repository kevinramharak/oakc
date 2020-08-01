use super::Target;
use std::{
    env::consts::EXE_SUFFIX,
    fs::{remove_file, write, read_to_string},
    fmt::{Debug, Display},
    io::{Error, ErrorKind, Result, Write},
    path::PathBuf,
    collections::BTreeMap,
    process::exit,
};

use asciicolor::Colorize;
use crate::{parse,asm::AsmError};

fn generate_stdlib(cwd: &PathBuf, input: String, target: impl Target) -> Result<String> {
    let mut hir = parse(input);

    match hir.compile(cwd, &target, &mut BTreeMap::new()) {
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

// TODO: figure out the rust way of using these safely
static mut unique: i16 = 0;
static mut loop_identifiers: Vec<i16> = Vec::new();

pub struct MAR;

impl Target for MAR {
    fn get_name(&self) -> char {
        'm'
    }

    fn std(&self) -> String {
        // we use the oak compiler itself to compile our stdlib
        // we kinda have to because MAR has no import system or compile time checks available
        // pathbuff from project root working directory?
        if let Ok(std) = generate_stdlib(&PathBuf::from("src/target/std/mar/"), String::from(include_str!("std/std.mar.ok")), MAR) {
            return std;
        }
        String::from(";; ERROR: could not generate the 'std.mar' file")
    }

    fn core_prelude(&self) -> String {
        String::from(include_str!("core/core.mar"))
    }

    fn core_postlude(&self) -> String {
        String::from("__CORE_INIT_HEAP_START:")
    }

    fn begin_entry_point(&self, var_size: i32, heap_size: i32) -> String {
        // TODO: constrain sizes and/or error
        // TODO: these should be constants, but because they are after core_prelude they cant be
        String::from(format!(r##"
__CORE_INIT_VM_VARS: DW {}
__CORE_INIT_VM_CAPACITY: DW {}
__core_main:
"##, var_size as i16, (heap_size + var_size) as i16))
    }

    fn end_entry_point(&self) -> String {
        String::from("    RET\n")
    }

    fn push(&self, n: f64) -> String {
        String::from(format!(
r##"    PUSH {}
    CALL __core_machine_push
"##, n as i16))
    }

    fn add(&self) -> String {
        String::from("    CALL __core_machine_add\n")
    }

    fn subtract(&self) -> String {
        String::from("    CALL __core_machine_subtract\n")
    }
    
    fn multiply(&self) -> String {
        String::from("    CALL __core_machine_multiply\n")
    }
    
    fn divide(&self) -> String {
        String::from("    CALL __core_machine_divide\n")
    }

    fn sign(&self) -> String {
        String::from("    CALL __core_machine_sign\n")
    }

    fn allocate(&self) -> String {
        String::from("    CALL __core_machine_allocate\n")
    }

    fn free(&self) -> String {
        String::from("    CALL __core_machine_free\n")
    }

    fn store(&self, size: i32) -> String {
        String::from(format!(
r##"    PUSH {}
    CALL __core_machine_store
"##, size as i16))
    }

    fn load(&self, size: i32) -> String {
        String::from(format!(
r##"    PUSH {}
    CALL __core_machine_load
"##, size as i16))
    }

    fn fn_header(&self, name: String) -> String {
        String::new()
    }

    fn fn_definition(&self, name: String, body: String) -> String {
        String::from(format!(r##"
{}:
{}    RET ;; returning from {}
"##, name, body, name))
    }

    fn call_fn(&self, name: String) -> String {
        String::from(format!("    CALL {} ;; calling oak function\n", name))
    }

    fn call_foreign_fn(&self, name: String) -> String {
        String::from(format!("    CALL {} ;; calling foreign function\n", name))
    }

    fn begin_while(&self) -> String {
        let id: i16 = unsafe { unique };
        unsafe { loop_identifiers.push(id); unique += 1; }
        let str = String::from(format!(
r#"begin_while_{}:
    CALL __core_machine_pop
    CMP A, 0
    JZ end_while_{}
"#, unsafe { id }, unsafe { id }));
        str
    }

    fn end_while(&self) -> String {
        let id = unsafe { loop_identifiers.pop().unwrap() };
        let str = String::from(format!(
r#"    JMP begin_while_{}
end_while_{}:
"#, unsafe { id }, unsafe { id }));
        str
    }

    fn compile(&self, code: String) -> Result<()> {
        if let Ok(_) = write("OUTPUT.mar", code) {
            return Result::Ok(())
        }
        return Result::Err(Error::new(ErrorKind::Other,
            "unabe to compile to MAR"));
    }
}
