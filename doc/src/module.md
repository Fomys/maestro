# Module

A kernel module allows to add a feature to the kernel without rebuild it.

This chapter discribes how to write a kernel module.



## Template

A basic kernel module contains the following files:

```
|- Makefile
|- src/
 |- mod.rs
```

These files are located in the `module.template` directory of the kernel's sources.

The Makefile must be modified to add the module's name in the `NAME` variable.

`mod.rs` is the file that contains the main functions of the module. It has the following structure.

```rust
#![no_std]

extern crate kernel;

use kernel::module::version::Version;
use kernel::print;

// hello module, version 1.0.0
kernel::module!("hello", Version::new(1, 0, 0));

/// Called on module load
#[no_mangle]
pub extern "C" fn init() {
	kernel::println!("Hello world!");
}

/// Called on module unload
#[no_mangle]
pub extern "C" fn fini() {
	kernel::println!("Goodbye!");
}
```

The `kernel` crate gives access to the kernel's functions.

The following properties have to be taken into account when writing a module:
- `init` is called once each times the module is loaded. The execution must be not block since it would freeze the system
- `fini` can be called at all times and must free every resources allocated by the module



## Building

The kernel must be built in its directory in order to be able to build the module.

To build a kernel module, first it is required to set the `KERN_SRC` environment variable to the path of the kernel's sources.

Then, just run `make` and the Makefile will produce the kernel module.