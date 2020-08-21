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
use crate::{parse,asm::AsmStatement, hir::{HirProgram, HirDeclaration}};

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

    // we made a hook into the hir program so we can include our 'std.mar.ok' file as if it were an include statement
    // this allows us to use the oak compile time features to generate the stdlib
    fn extend_hir(&self, cwd: &PathBuf, hir: &mut HirProgram) {
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
            let mut decls = vec!(HirDeclaration::Include(String::from(diff.to_str().unwrap())));
            hir.extend_declarations(&decls);
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
        // technically we want to get the return value from main and return it to the hosting environment
        // but since the target implementation is the host, we can do whatever we want here
        String::from(r"
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
    n as i16)
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
__CORE_INIT_VM_CAPACITY equ {}",
        self.global_scope_size.get(),
        self.init_vm_capacity.get()) + &code;

        if let Ok(_) = write("main.mar", asm) {
            return Result::Ok(())
        }
        return Result::Err(Error::new(ErrorKind::Other, "unabe to compile to MAR"));
    }
}
