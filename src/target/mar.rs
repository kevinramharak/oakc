use super::Target;
use std::{
    env::consts::EXE_SUFFIX,
    fs::{remove_file, write, read_to_string},
    fmt::{Debug, Display},
    io::{Error, ErrorKind, Result, Write},
    path::{Path, PathBuf},
    convert::TryFrom,
    collections::BTreeMap,
    process::exit,
    cell::{Cell, RefCell},
};

use pathdiff::diff_paths;
use asciicolor::Colorize;
use parse_int;
use crate::{parse,asm::AsmStatement, hir::HirProgram, tir::{TirProgram, TirDeclaration, TirConstant}};

pub struct MAR {
    global_scope_size: Cell<u16>,
    init_vm_capacity: Cell<u16>,
    unique_id: Cell<u16>,
    loop_identifiers: RefCell<Vec<u16>>,
    is_std: Cell<bool>,
}

impl MAR {
    fn generate_id(&self) -> u16 {
        let id = self.unique_id.get();
        self.unique_id.set(id + 1);
        return id;
    }

    fn link_asm(&self, asm: String) -> String {
        let mut result: Vec<String> = Vec::new();
        let mut initializers: Vec<String> = Vec::new();
        let mut destructors: Vec<String> = Vec::new();

        // generates tables at the lines containing these flags, expect only 1 entry with the maximum size right after the flag seperated by a space
        let generate_initializer_table_flag = ";; %[generate_initializer_table] ";
        let generate_destructor_table_flag = ";; %[generate_destructor_table] ";
        let mut initializer_table_index: usize = 0;
        let mut initializer_table_size: usize = 0;
        let mut destructor_table_index: usize = 0;
        let mut destructor_table_size: usize = 0;

        // registers lines starting with these flags as initializers/destructors. The function name should be following right after the flag seperated by a space
        let initializer_flag = ";; %[runtime_initializer] ";
        let destructor_flag = ";; %[runtime_destructor] ";

        let mut iterator = asm.lines().enumerate();
        while let Some((index, line)) = iterator.next() {
            if line.contains(generate_initializer_table_flag) {
                if initializer_table_index != 0 {
                    panic!("cannot have 2 instances of the '{}' flag", generate_initializer_table_flag);
                } else {
                    initializer_table_index = index + 1;
                    let slice = line.trim()[generate_initializer_table_flag.len()..].trim();
                    if let Ok(table_size) = parse_int::parse::<usize>(slice) {
                        initializer_table_size = table_size;
                    } else {
                        panic!("invalid initializer_table_size in '{}'", slice);
                    }
                    // discard the next  2 lines as it is the padding reserved for the table
                    iterator.next();
                    iterator.next();
                }
            } else if line.contains(generate_destructor_table_flag) {
                if destructor_table_index != 0 {
                    panic!("cannot have 2 instances of the '{}' flag", generate_destructor_table_flag);
                } else {
                    destructor_table_index = index + 1;
                    let slice = line.trim()[generate_destructor_table_flag.len()..].trim();
                    if let Ok(table_size) = parse_int::parse::<usize>(slice) {
                        destructor_table_size = table_size;
                    } else {
                        panic!("invalid destructor_table_size in '{}'", slice);
                    }
                    // discard the next 2 lines as it is the padding reserved for the table
                    iterator.next();
                    iterator.next();
                }
            } else if line.contains(initializer_flag) {
                let name = &line[initializer_flag.len()..];
                initializers.push(String::from(name));
            } else if line.contains(destructor_flag) {
                let name = &line[destructor_flag.len()..];
                destructors.push(String::from(name));
            }
            // always want to insert the line to make it easier to debug
            result.push(String::from(line));
        }

        // generate intializer table
        result.insert(initializer_table_index, format!("    __core_initializer_vector_table_length: dw {}", initializers.len()));
        initializer_table_index += 1;
        destructor_table_index += 1;
        let initializers_length = initializers.len();
        result.insert(initializer_table_index, String::from("    __core_initializer_vector_table_entries:"));
        if initializers_length > 0 {
            initializer_table_index += 1;
            destructor_table_index += 1;
            for initializer in initializers {
                result.insert(initializer_table_index, format!("    dw {}", initializer));
                destructor_table_index += 1;
                initializer_table_index += 1;
            }
        }
        let initializer_table_padding = initializer_table_size - initializers_length;
        if (initializer_table_padding > 0) {
            result.insert(initializer_table_index, format!("    dw {} dup(0xdead);; remaining padding", initializer_table_padding));
            destructor_table_index += 1;
        }

        // generate destructor table
        result.insert(destructor_table_index, format!("    __core_destructor_vector_table_length: dw {}", destructors.len()));
        destructor_table_index += 1;
        let destructors_length = destructors.len();
        result.insert(destructor_table_index, String::from("    __core_destructor_vector_table_entries:"));
        if destructors_length > 0 {
            destructor_table_index += 1;
            for destructor in destructors {
                result.insert(destructor_table_index, format!("    dw {}", destructor));
                destructor_table_index += 1;
            }
        }
        let destructor_table_padding = destructor_table_size - destructors_length;
        if (destructor_table_padding > 0) {
            result.insert(destructor_table_index, format!("    dw {} dup(0xdead);; remaining padding", destructor_table_padding));
        }

        
        return result.join("\n");
    }
}

impl Default for MAR {
    fn default() -> Self {
        MAR { 
            global_scope_size: Cell::new(0),
            init_vm_capacity: Cell::new(0),
            unique_id: Cell::new(0),
            loop_identifiers: RefCell::new(Vec::new()),
            is_std: Cell::new(false),
        }
    }
}

impl Target for MAR {
    fn get_name(&self) -> char {
        'm'
    }

    fn is_standard(&self) -> bool {
        false
    }

    // we made a hook into the hir program so we can include our 'std.mar.ok' file as if it were an include statement
    // this allows us to use the oak compile time features to generate the stdlib
    fn extend_hir(&self, cwd: &PathBuf, constants: &mut BTreeMap<String, TirConstant>, hir: &mut HirProgram) {
        let is_std = hir.use_std();
        if is_std {
            self.is_std.set(is_std);
            let cwd_path = cwd.canonicalize().unwrap();
            let file = file!();
            let mut path = PathBuf::from(file);
            path.pop();
            path.push("std");
            path.push("std.mar.ok");
            path = path.canonicalize().unwrap();
            let diff = diff_paths(&path, &cwd_path).unwrap();
            let mut decls = vec!(TirDeclaration::Include(String::from(diff.to_str().unwrap())));
            let mut program = TirProgram::new(decls, 0);
            match program.compile(cwd, constants) {
                Ok(std_hir) => {
                    hir.extend_declarations(std_hir.get_declarations());
                }
                Err(error) => {
                    panic!(error);
                }
            }
        }
    }

    fn std(&self) -> String {
        String::new()
    }

    fn core_prelude(&self) -> String {
        String::from(include_str!("core/core.mar"))
    }

    fn core_postlude(&self) -> String {
        String::from(r"
__core_heap_start: ;; heap starts at this address")
    }

    fn begin_entry_point(&self, global_scope_size: i32, memory_size: i32) -> String {
        // because of how constants need to be manually hoisted in MAR assembly
        // we save these values and prefix them to the code in the compile function
        self.global_scope_size.set(u16::try_from(global_scope_size).ok().unwrap());
        self.init_vm_capacity.set(u16::try_from(memory_size).ok().unwrap());
        format!(r"
;; start of entry point
__core_main:")
    }

    fn end_entry_point(&self) -> String {
        // TODO: if main() is able to return a value we should handle it here
        String::from(r"
    mov a, 0 ;; return 0 by default
    ret ;; return from entry point")
    }

    fn establish_stack_frame(&self, arg_size: i32, local_scope_size: i32) -> String {
        format!(r"
    push {} ;; local_scope_size
    push {} ;; arg_size
    call __core_machine_establish_stack_frame",
    u16::try_from(local_scope_size).ok().unwrap(),
    u16::try_from(arg_size).ok().unwrap())
    }

    fn end_stack_frame(&self, return_size: i32, local_scope_size: i32) -> String {
        format!(r"
    push {} ;; local_scope_size
    push {} ;; return size
    call __core_machine_end_stack_frame",
    u16::try_from(local_scope_size).ok().unwrap(),
    u16::try_from(return_size).ok().unwrap())
    }

    fn load_base_ptr(&self) -> String {
        String::from(r"
    call __core_machine_load_base_ptr ;; push the base pointer on the stack")
    }

    fn push(&self, n: f64) -> String {
        // TODO: i16::try_from><f64>() is not implemented? kinda want to do a checked cast here
        format!(r"
    push {} ;; push value on the vm stack
    call __core_machine_push",
    n as u16)
    }

    fn add(&self) -> String {
        String::from(r"
    call __core_machine_add")
    }

    fn subtract(&self) -> String {
        String::from(r"
    call __core_machine_subtract")
    }
    
    fn multiply(&self) -> String {
        String::from(r"
    call __core_machine_multiply")
    }
    
    fn divide(&self) -> String {
        String::from("
    call __core_machine_divide")
    }

    fn sign(&self) -> String {
        String::from(r"
    call __core_machine_sign")
    }

    fn allocate(&self) -> String {
        String::from(r"
    call __core_machine_allocate")
    }

    fn free(&self) -> String {
        String::from(r"
    call __core_machine_free")
    }

    fn store(&self, size: i32) -> String {
        format!(r"
    push {} ;; size
    call __core_machine_store
", u16::try_from(size).ok().unwrap())
    }

    fn load(&self, size: i32) -> String {
        format!(r"
    push {} ;; size
    call __core_machine_load
", u16::try_from(size).ok().unwrap())
    }

    fn fn_header(&self, name: String) -> String {
        String::new()
    }

    fn fn_definition(&self, name: String, body: String) -> String {
        format!(r"
{}:       ;; definition of {}
{}
    ret ;; returning from {}",
    name, name, body, name)
    }

    fn call_fn(&self, name: String) -> String {
        format!(r"
    call {} ;; calling oak function",
    name)
    }

    fn call_foreign_fn(&self, name: String) -> String {
        format!(r"
    call {} ;; calling foreign function",
        name)
    }

    fn begin_while(&self) -> String {
        let id = self.generate_id();
        self.loop_identifiers.borrow_mut().push(id);
        format!(r"
__generated_begin_while_{}:
    call __core_machine_pop
    cmp A, 0
    jz __generated_end_while_{}",
        id, id)
    }

    fn end_while(&self) -> String {
        let id = self.loop_identifiers.borrow_mut().pop().unwrap();
        format!(r"
    jmp __generated_begin_while_{}
__generated_end_while_{}:",
        id, id)
    }

    fn compile(&self, code: String) -> Result<()> {
        // prefix the saved values as constants
        let mut asm = format!(r"
__CORE_GLOBAL_SCOPE_SIZE equ {}
__CORE_INIT_VM_CAPACITY equ {}
",
        self.global_scope_size.get(),
        self.init_vm_capacity.get()) + &code;

        // since we need a 'linker' we implemented a simple one in rust
        asm = self.link_asm(asm);

        if let Ok(_) = write("main.mar", asm) {
            return Result::Ok(())
        }
        return Result::Err(Error::new(ErrorKind::Other, "unabe to compile to MAR"));
    }
}
