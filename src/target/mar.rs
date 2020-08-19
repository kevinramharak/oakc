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
        }
    }
}

impl Target for MAR {
    fn get_name(&self) -> char {
        'm'
    }

    // we made a hook into the hir program so we can include our 'std.mar.ok' file as if it were an include statement
    // this allows us to use the oak compile time features to generate the mar stdlib
    fn extend_hir(&self, cwd: &PathBuf, hir: &mut HirProgram) {
        if hir.use_std() {
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
        String::from("\n__core_heap_start: ;; heap starts at this address")
    }

    fn begin_entry_point(&self, global_scope_size: i32, memory_size: i32) -> String {
        // because of how constants need to be manually hoisted in MAR assembly
        // we save these values and prefix them to the code in the compile function
        self.global_scope_size.set(u16::try_from(global_scope_size).unwrap());
        self.init_vm_capacity.set(u16::try_from(memory_size).unwrap());
        String::from(format!(r"
__core_main:"))
    }

    fn end_entry_point(&self) -> String {
        format!(r"
    ret")
    }

    fn establish_stack_frame(&self, arg_size: i32, local_scope_size: i32) -> String {
        let id = self.generate_id();
        format!(r"
    mov b, sp ;; sp/b should point to the return address
    sub b, 2  ;; allocate 1 word for the base pointer and 1 for the return address
    sub b, {} ;; allocate space for local vars
    mov c, {} ;; arg_size
    mov x, b
    pop y     ;; save return address in y
__label_start_{}:
    cmp c, 0
    jz __label_done_{}
    pop [b]
    inc b
    dec c
    jmp __label_start_{}
__label_done_{}:
    push y     ;; put return address back on the stack
    push bp    ;; save bp
    mov bp, sp ;; create a stack frame
    mov sp, x  ;; point sp to the first arg",
    u16::try_from(local_scope_size).unwrap(),
    u16::try_from(arg_size).unwrap(),
    id, id, id, id)
    }

    fn end_stack_frame(&self, return_size: i32, local_scope_size: i32) -> String {
        let id = self.generate_id();
        format!(r"
    mov b, bp ;; b should point to the old bp
    sub b, {} ;; allocate space for return value
    mov c, {} ;; return_size
    mov [b], [bp]         ;; move old bp up the stack
    mov [b + 1], [bp + 1] ;; move return address up the stack
    mov bp, b
    add b, 2
__label_start_{}:         ;; move the return value right after the return address
    cmp c, 0              ;; this would put it on top of the stack executing the RET instruction
    jz __label_done_{}
    pop [b]
    inc b
    dec c
    jmp __label_start_{}
__label_done_{}:
    mov sp, bp ;; restore stack frame
    pop bp     ;; restore old bp",
    u16::try_from(local_scope_size).unwrap(),
    u16::try_from(return_size).unwrap(),
    id, id, id, id)
    }

    fn load_base_ptr(&self) -> String {
        format!(r"
    push bp")
    }

    fn push(&self, n: f64) -> String {
        format!(r"
    push {}", n as i16)
    }

    fn add(&self) -> String {
format!(r"
    pop b
    pop a
    add a, b
    push a")
    }

    fn subtract(&self) -> String {
format!(r"
    pop b
    pop a
    sub a, b
    push a")
    }
    
    fn multiply(&self) -> String {
format!(r"
    pop b
    pop a
    mul b ;; A * B = Y:A
    push a")
    }
    
    fn divide(&self) -> String {
format!(r"
    xor y, y
    pop b
    pop a
    div b ;; Y:A / B = A && Y:A % B = Y
    push a")
    }

    fn sign(&self) -> String {
format!(r"
    pop a
    cmp a, 0
    setz a
    push a")
    }

    fn allocate(&self) -> String {
        todo!()
    }

    fn free(&self) -> String {
        todo!()
    }

    fn store(&self, size: i32) -> String {
        let id = self.generate_id();
        format!(r"
    pop c ;; address
    mov b, {} ;; size
    dec b
__label_start_{}:
    cmp b, 0
    jl __label_done_{}
    mov d, c
    add d, b
    pop [d]
    dec b
    jmp __label_start_{}
__label_done_{}:",
    u16::try_from(size).unwrap(),
    id, id, id, id)
    }

    fn load(&self, size: i32) -> String {
        let id = self.generate_id();
        format!(r"
    pop c ;; address
    mov b, {} ;; size
    xor x, x
__label_start_{}:
    cmp b, x
    jge __label_done_{}
    mov d, c
    add d, b
    push [d]
    inc b
    jmp __label_start_{}
__label_done_{}:",
    u16::try_from(size).unwrap(),
    id, id, id, id)
    }

    fn fn_header(&self, name: String) -> String {
        String::new()
    }

    fn fn_definition(&self, name: String, body: String) -> String {
        format!(r"
{}:
{}
    ret", name, body)
    }

    fn call_fn(&self, name: String) -> String {
        format!(r"
    call {}", name)
    }

    fn call_foreign_fn(&self, name: String) -> String {
        format!(r"
    call {} ;; foreign function call", name)
    }

    fn begin_while(&self) -> String {
        let id = self.generate_id();
        self.loop_identifiers.borrow_mut().push(id);
        format!(r"
__generated_begin_while_{}:
    pop a
    cmp A, 0
    jz __generated_end_while_{}", id, id)
    }

    fn end_while(&self) -> String {
        let id = self.loop_identifiers.borrow_mut().pop().unwrap();
        format!(r"
    jmp __generated_begin_while_{}
__generated_end_while_{}:", id, id)
    }

    fn compile(&self, code: String) -> Result<()> {
        // prefix the saved values as constants
        let code_with_prefixed_constants = String::from(format!(
r#"
__CORE_GLOBAL_SCOPE_SIZE equ {}
__CORE_INIT_VM_CAPACITY equ {}
"#, self.global_scope_size.get(), self.init_vm_capacity.get())) + code.as_str();
        if let Ok(_) = write("OUTPUT.mar", code_with_prefixed_constants) {
            return Result::Ok(())
        }
        return Result::Err(Error::new(ErrorKind::Other,
            "unabe to compile to MAR"));
    }
}
