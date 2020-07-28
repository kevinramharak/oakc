use super::Target;
use std::{
    env::consts::EXE_SUFFIX,
    fs::{remove_file, write, read},
    io::{Error, ErrorKind, Result, Write},
    process::Command,
};

static mut unique: i16 = 0;
static mut nested: i16 = 0;

pub struct MAR;

impl Target for MAR {
    fn prelude(&self) -> String {
        let buffer = read("src/target/std.mar").unwrap();
        String::from_utf8(buffer).unwrap()
    }

    fn postlude(&self) -> String {
        String::new()
    }

    fn begin_entry_point(&self, var_size: i32, heap_size: i32) -> String {
        String::from(format!(r##"
_machine_new_vars: DW {}
_machine_new_capacity: DW {}
main:
"##, var_size as i16, (heap_size + var_size) as i16))
    }

    fn end_entry_point(&self) -> String {
        String::from(
r##"    RET
_HEAP_START:
"##)
    }

    fn push(&self, n: f64) -> String {
        String::from(format!(
r##"    PUSH {}
    CALL machine_push
"##, n as i16))
    }

    fn add(&self) -> String {
        String::from("    CALL machine_add\n")
    }

    fn subtract(&self) -> String {
        String::from("    CALL machine_subtract\n")
    }

    fn multiply(&self) -> String {
        String::from("    CALL machine_multiply\n")
    }

    fn divide(&self) -> String {
        String::from("    CALL machine_divide\n")
    }

    fn allocate(&self) -> String {
        String::from("    CALL machine_allocate\n")
    }

    fn free(&self) -> String {
        String::from("    CALL machine_free\n")
    }

    fn store(&self, size: i32) -> String {
        String::from(format!(
r##"    PUSH {}
    CALL machine_store
"##, size as i16))
    }

    fn load(&self, size: i32) -> String {
        String::from(format!(
r##"    PUSH {}
    CALL machine_load
"##, size as i16))
    }

    fn fn_header(&self, name: String) -> String {
        String::new()
    }

    fn fn_definition(&self, name: String, body: String) -> String {
        String::from(format!(r##"
{}:
{}    RET ; COMPILER: return from {}
"##, name, body, name))
    }

    fn call_fn(&self, name: String) -> String {
        String::from(format!("    CALL {}\n", name))
    }

    fn call_foreign_fn(&self, name: String) -> String {
        String::from(format!("    CALL {}\n", name))
    }

    fn begin_while(&self) -> String {
        unsafe { nested += 1; }
        let str = String::from(format!(
r#"begin_while_{}:
    CALL machine_pop
    CMP A, 0
    JZ end_while_{}
"#, unsafe { unique }, unsafe { unique }));
        unsafe { unique += 1 }
        str
    }

    fn end_while(&self) -> String {
        let str = String::from(format!(
r#"   JMP begin_while_{}
end_while_{}:
"#, unsafe { unique - nested }, unsafe { unique - nested }));
        unsafe { nested -= 1; }
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
