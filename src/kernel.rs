//! Maestro is a Unix kernel written in Rust. This reference documents
//! interfaces for modules and the kernel's internals.
//!
//! # Features
//!
//! The crate has the following features:
//! - `strace`: if enabled, the kernel traces system calls. This is a debug feature.

#![feature(asm_const)]
#![no_std]
#![allow(internal_features)]
#![no_main]
#![feature(allow_internal_unstable)]
#![feature(array_chunks)]
#![feature(associated_type_defaults)]
#![feature(coerce_unsized)]
#![feature(core_intrinsics)]
#![feature(custom_test_frameworks)]
#![feature(dispatch_from_dyn)]
#![feature(exclusive_range_pattern)]
#![feature(is_sorted)]
#![feature(iterator_try_collect)]
#![feature(lang_items)]
#![feature(nonzero_ops)]
#![feature(offset_of)]
#![feature(panic_info_message)]
#![feature(pointer_is_aligned)]
#![feature(ptr_metadata)]
#![feature(slice_index_methods)]
#![feature(stmt_expr_attributes)]
#![feature(strict_provenance)]
#![feature(trait_upcasting)]
#![feature(trusted_len)]
#![feature(unsize)]
#![feature(set_ptr_value)]
#![deny(warnings)]
#![allow(unused_attributes)]
#![allow(dead_code)]
#![allow(unused_macros)]
#![allow(incomplete_features)]
#![test_runner(crate::selftest::runner)]
#![reexport_test_harness_main = "kernel_selftest"]

pub mod acpi;
pub mod cmdline;
pub mod cpu;
pub mod crypto;
pub mod debug;
pub mod device;
pub mod elf;
#[macro_use]
pub mod errno;
pub mod event;
pub mod file;
#[cfg(target_arch = "x86")]
pub mod gdt;
#[macro_use]
pub mod idt;
pub mod io;
pub mod limits;
pub mod logger;
pub mod memory;
pub mod module;
pub mod multiboot;
pub mod net;
#[macro_use]
pub mod panic;
pub mod power;
#[macro_use]
pub mod print;
pub mod process;
pub mod selftest;
pub mod syscall;
pub mod time;
pub mod tty;
#[macro_use]
pub mod util;
#[macro_use]
pub mod vga;

use crate::errno::Errno;
use crate::file::fs::initramfs;
use crate::file::path::Path;
use crate::file::perm::AccessProfile;
use crate::file::vfs;
use crate::logger::LOGGER;
use crate::memory::vmem;
use crate::memory::vmem::VMem;
use crate::process::exec;
use crate::process::exec::ExecInfo;
use crate::process::Process;
use crate::util::boxed::Box;
use crate::util::container::string::String;
use crate::util::container::vec::Vec;
use crate::util::lock::Mutex;
use core::arch::asm;
use core::ffi::c_void;
use core::ptr::null;

/// The kernel's name.
pub const NAME: &str = env!("CARGO_PKG_NAME");
/// Current kernel version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The name of the current architecture.
pub const ARCH: &str = "x86";

/// The path to the init process binary.
const INIT_PATH: &[u8] = b"/sbin/init";

/// The current hostname of the system.
pub static HOSTNAME: Mutex<Vec<u8>> = Mutex::new(Vec::new());

extern "C" {
	fn kernel_loop_reset(stack: *mut c_void) -> !;
}

/// Makes the kernel wait for an interrupt, then returns.
/// This function enables interruptions.
#[inline(always)]
pub fn wait() {
	unsafe {
		asm!("sti", "hlt");
	}
}

/// Enters the kernel loop and processes every interrupts indefinitely.
pub fn enter_loop() -> ! {
	loop {
		wait();
	}
}

/// Resets the stack to the given value, then calls [`enter_loop`].
///
/// The function is unsafe because the pointer passed in parameter might be
/// invalid.
pub unsafe fn loop_reset(stack: *mut c_void) -> ! {
	kernel_loop_reset(stack);
}

/// Field storing the kernel's virtual memory context.
static KERNEL_VMEM: Mutex<Option<Box<dyn VMem>>> = Mutex::new(None);

/// Initializes the kernel's virtual memory context.
fn init_vmem() -> Result<(), Errno> {
	let kernel_vmem = vmem::new()?;

	// TODO If Meltdown mitigation is enabled, only allow read access to a stub of
	// the kernel for interrupts

	// TODO Enable GLOBAL in cr4

	// Mapping the kernelspace
	kernel_vmem.map_range(
		null::<c_void>(),
		memory::PROCESS_END,
		memory::get_kernelspace_size() / memory::PAGE_SIZE,
		vmem::x86::FLAG_WRITE,
	)?;

	// Mapping VGA's buffer
	let vga_flags = vmem::x86::FLAG_CACHE_DISABLE
		| vmem::x86::FLAG_WRITE_THROUGH
		| vmem::x86::FLAG_WRITE
		| vmem::x86::FLAG_GLOBAL;
	kernel_vmem.map_range(
		vga::BUFFER_PHYS as _,
		vga::get_buffer_virt() as _,
		1,
		vga_flags,
	)?;

	// Making the kernel image read-only
	kernel_vmem.protect_kernel()?;

	// Assigning to the global variable
	*KERNEL_VMEM.lock() = Some(kernel_vmem);

	// Binding the kernel virtual memory context
	bind_vmem();
	Ok(())
}

/// Returns the kernel's virtual memory context.
pub fn get_vmem() -> &'static Mutex<Option<Box<dyn VMem>>> {
	&KERNEL_VMEM
}

/// Tells whether memory management has been fully initialized.
pub fn is_memory_init() -> bool {
	get_vmem().lock().is_some()
}

/// Binds the kernel's virtual memory context.
///
/// If the kernel vmem is not initialized, the function does nothing.
pub fn bind_vmem() {
	let guard = KERNEL_VMEM.lock();

	if let Some(vmem) = guard.as_ref() {
		vmem.bind();
	}
}

/// Launches the init process.
///
/// `init_path` is the path to the init program.
fn init(init_path: String) -> Result<(), Errno> {
	let path = Path::from_str(&init_path, true)?;

	let proc_mutex = Process::new()?;
	let mut proc = proc_mutex.lock();

	// The initial environment
	let env: Vec<String> = vec![
		b"PATH=/bin:/sbin:/usr/bin:/usr/sbin:/usr/local/bin:/usr/local/sbin".try_into()?,
		b"TERM=maestro".try_into()?,
	]?;

	let file_mutex = vfs::get_file_from_path(&path, &AccessProfile::KERNEL, true)?;
	let mut file = file_mutex.lock();

	let exec_info = ExecInfo {
		access_profile: AccessProfile::KERNEL,
		argv: vec![init_path]?,
		envp: env,
	};
	let program_image = exec::build_image(&mut file, exec_info)?;

	exec::exec(&mut proc, program_image)
}

/// This is the main function of the Rust source code, responsible for the
/// initialization of the kernel.
///
/// When calling this function, the CPU must be in Protected Mode with the GDT loaded with space
/// for the Task State Segment.
///
/// Arguments:
/// - `magic` is the magic number passed by Multiboot.
/// - `multiboot_ptr` is the pointer to the Multiboot booting informations
/// structure.
#[no_mangle]
pub extern "C" fn kernel_main(magic: u32, multiboot_ptr: *const c_void) -> ! {
	// Initializing TTY
	tty::init();

	if magic != multiboot::BOOTLOADER_MAGIC || !multiboot_ptr.is_aligned_to(8) {
		panic!("Bootloader non compliant with Multiboot2!");
	}

	// Initializing IDT
	idt::init();

	// Ensuring the CPU has SSE
	if !cpu::sse::is_present() {
		panic!("SSE support is required to run this kernel :(");
	}
	cpu::sse::enable();

	// Reading multiboot informations
	multiboot::read_tags(multiboot_ptr);

	// Initializing memory allocation
	memory::memmap::init(multiboot_ptr);
	if cfg!(config_debug_debug) {
		memory::memmap::print_entries();
	}
	memory::alloc::init();

	if init_vmem().is_err() {
		panic!("Cannot initialize kernel virtual memory!");
	}

	// From here, the kernel considers that memory management has been fully
	// initialized

	// Performing kernel self-tests
	#[cfg(test)]
	kernel_selftest();

	let boot_info = multiboot::get_boot_info();

	// Parsing bootloader command line arguments
	let cmdline = boot_info.cmdline.unwrap_or(b"");
	let args_parser = match cmdline::ArgsParser::parse(cmdline) {
		Ok(p) => p,
		Err(e) => {
			println!("{e}");
			power::halt();
		}
	};
	LOGGER.lock().silent = args_parser.is_silent();

	println!("Booting Maestro kernel version {VERSION}");

	// FIXME
	//println!("Initializing ACPI...");
	//acpi::init();

	println!("Initializing time management...");
	if time::init().is_err() {
		panic!("failed to initialize time management");
	}

	// FIXME
	/*println!("Initializing ramdisks...");
	device::storage::ramdisk::create()
		.unwrap_or_else(|e| kernel_panic!("Failed to create ramdisks! ({})", e));*/
	println!("Initializing devices management...");
	device::init().unwrap_or_else(|e| panic!("Failed to initialize devices management! ({e})"));
	net::osi::init().unwrap_or_else(|e| panic!("Failed to initialize network! ({e})"));
	crypto::init().unwrap_or_else(|e| panic!("Failed to initialize cryptography! ({e})"));

	let root = args_parser.get_root_dev();
	println!("Initializing files management...");
	file::init(root).unwrap_or_else(|e| panic!("Failed to initialize files management! ({e})"));
	if let Some(initramfs) = &boot_info.initramfs {
		println!("Initializing initramfs...");
		initramfs::load(initramfs)
			.unwrap_or_else(|e| panic!("Failed to initialize initramfs! ({e})"));
	}
	device::stage2().unwrap_or_else(|e| panic!("Failed to create device files! ({e})"));

	println!("Initializing processes...");
	process::init().unwrap_or_else(|e| panic!("Failed to init processes! ({e})"));

	let init_path = args_parser.get_init_path().unwrap_or(INIT_PATH);
	let init_path = String::try_from(init_path).unwrap();
	init(init_path).unwrap_or_else(|e| panic!("Cannot execute init process: {e}"));

	drop(args_parser);
	enter_loop();
}
