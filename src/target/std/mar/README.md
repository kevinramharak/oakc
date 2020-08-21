# Calling convention

## registers
- A - return value - caller saved
- B - callee saved
- C - callee saved
- D - callee saved
- X - callee saved
- Y - callee saved
- BP
- SP

## `std/mar` directory structure
```
+ std
  + mar
    + c
    + ffi
    + hwi
    + oak
    - README.md
  - std.mar.ok
```
### why `*.ok` files
We use `.ok` files to make use of the oak compiler to generate the stdlib during compile time. This provides include guards, include statements, assembly includes with `#[extern("file.mar")]` and defining `ffi` functions to use in `oak` code.

### `std.mar.ok`
The entry point of the oak stdlib.

### `c`
The C stdlib implementation.

### `hwi`
Simple wrappers to provide `ffi` functions to directly execute the `HWI` instructions in `oak` code.

### `internal`
Helper functions to abstract the internal architecture to something more generic. These functions are used to implement the `c` stdlib

### `oak`
The implementation of the oak stdlib.

# todos
- document setcc instructions on github
- finish https://github.com/simon987/Much-Assembly-Required/pull/205
- use setcc instructions in core/std
- document/implement lea instructions on github
- make sure the calling convention is honored
- implement a linking phase so std can hook into the runtime initialize phase
- implement basic stuff of stdlib
- implement threading/coroutine library with setjmp/longjmp, also with stacks saved into the floppy disk?
- implement filesystem
- implement kernel
- implement shell
- generate oak program as if its a `main` executable in cwd when opening the shell? idno gets weird
- draw out the architecture/memory layout