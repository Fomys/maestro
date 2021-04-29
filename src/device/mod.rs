/// This module handles device and buses.
/// A device file is an interface with a device of the system, which can be internal or external,
/// or even virtual such as a TTY.

pub mod bus;
pub mod default;
pub mod id;
pub mod ps2;
pub mod storage;

use core::cmp::Ordering;
use crate::errno::Errno;
use crate::filesystem::Mode;
use crate::filesystem::path::Path;
use crate::module::Module;
use crate::util::boxed::Box;
use crate::util::container::vec::Vec;
use crate::util::lock::mutex::Mutex;
use crate::util::lock::mutex::MutexGuard;
use storage::StorageInterface;
use storage::pata::PATAInterface;

/// Enumeration representing the type of the device.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DeviceType {
	/// A block device.
	Block,
	/// A char device.
	Char,
}

/// Trait providing a interface for device I/O.
pub trait DeviceHandle {
	/// Reads data from the device and writes it to the buffer `buff`.
	/// `offset` is the offset in the file.
	/// The function returns the number of bytes read.
	fn read(&mut self, offset: usize, buff: &mut [u8]) -> Result<usize, Errno>;
	/// Writes data to the device, reading it from the buffer `buff`.
	/// `offset` is the offset in the file.
	/// The function returns the number of bytes written.
	fn write(&mut self, offset: usize, buff: &[u8]) -> Result<usize, Errno>;
}

/// Structure representing a device, either a block device or a char device. Each device has a
/// major and a minor number.
pub struct Device {
	/// The major number.
	major: u32,
	/// The minor number.
	minor: u32,

	/// The path to the device file.
	path: Path,
	/// The file's mode.
	mode: Mode,
	/// The type of the device.
	type_: DeviceType,

	/// The object handling the device I/O.
	handle: Box::<dyn DeviceHandle>,
}

impl Device {
	/// Creates a new instance.
	/// `major` and `minor` are the major and minor numbers of the device.
	/// `type_` is the type of the device.
	/// `handle` is the handle for I/O operations.
	pub fn new<H: 'static + DeviceHandle>(major: u32, minor: u32, path: Path, mode: Mode,
		type_: DeviceType, handle: H)
		-> Result<Self, Errno> {
		Ok(Self {
			major: major,
			minor: minor,

			path: path,
			mode: mode,
			type_: type_,

			handle: Box::new(handle)?,
		})
	}

	/// Returns the major number.
	pub fn get_major(&self) -> u32 {
		self.major
	}

	/// Returns the minor number.
	pub fn get_minor(&self) -> u32 {
		self.minor
	}

	/// Returns the path to the device file.
	pub fn get_path(&self) -> &Path {
		&self.path
	}

	/// Returns the device file's mode.
	pub fn get_mode(&self) -> Mode {
		self.mode
	}

	/// Returns the type of the device.
	pub fn get_type(&self) -> DeviceType {
		self.type_
	}

	/// Returns the device number.
	pub fn get_device_number(&self) -> u64 {
		id::makedev(self.major, self.minor)
	}

	/// Returns the handle of the device for I/O operations.
	pub fn get_handle(&mut self) -> &mut dyn DeviceHandle {
		self.handle.as_mut()
	}
}

/// The list of registered block devices.
static mut BLOCK_DEVICES: Mutex::<Vec::<Mutex::<Device>>> = Mutex::new(Vec::new());
/// The list of registered block devices.
static mut CHAR_DEVICES: Mutex::<Vec::<Mutex::<Device>>> = Mutex::new(Vec::new());

/// Registers the given device. If the minor/major number is already used, the function fails.
pub fn register_device(device: Device) -> Result<(), ()> {
	let mut guard = match device.get_type() {
		DeviceType::Block => {
			unsafe { // Safe because using mutex
				MutexGuard::new(&mut BLOCK_DEVICES)
			}
		},
		DeviceType::Char => {
			unsafe { // Safe because using mutex
				MutexGuard::new(&mut CHAR_DEVICES)
			}
		}
	};
	let container = guard.get_mut();

	let device_number = device.get_device_number();
	let index = container.binary_search_by(| d | {
		let dn = unsafe { // Safe because reading values that cannot be modified
			d.get_payload().get_device_number()
		};

		if device_number < dn {
			Ordering::Less
		} else if device_number > dn {
			Ordering::Greater
		} else {
			Ordering::Equal
		}
	});
	let index = match index {
		Ok(i) => i,
		Err(i) => i,
	};

	// TODO Add new device file

	if container.insert(index, Mutex::new(device)).is_ok() {
		Ok(())
	} else {
		Err(())
	}
}

// TODO Function to remove a device

/*
/// Returns a mutable reference to the device with the given major number, minor number and type.
pub fn get_device(major: u32, minor: u32, type_: DeviceType)
	-> Option<&'static mut Mutex<Device>> {
	let mut guard = match type_ {
		DeviceType::Block => {
			unsafe { // Safe because using mutex
				MutexGuard::new(&mut BLOCK_DEVICES)
			}
		},
		DeviceType::Char => {
			unsafe { // Safe because using mutex
				MutexGuard::new(&mut CHAR_DEVICES)
			}
		}
	};
	let container = guard.get_mut();

	let device_number = makedev(major, minor);
	let index = container.binary_search_by(| d | {
		let dn = unsafe { // Safe because reading values that cannot be modified
			d.get_payload().get_device_number()
		};

		if device_number < dn {
			Ordering::Less
		} else if device_number > dn {
			Ordering::Greater
		} else {
			Ordering::Equal
		}
	});

	if let Ok(i) = index {
		Some(&mut container[i])
	} else {
		None
	}
}*/

// TODO
/// Initializes PS/2 devices.
fn init_ps2() {
	let mut ps2_module = ps2::PS2Module::new(| c, action | {
		crate::println!("Key action! {:?} {:?}", c, action);
		// TODO Write to device file
	});
	if ps2_module.init().is_err() {
		crate::kernel_panic!("Failed to init PS/2 kernel module!", 0);
	}
}

/// Initializes PATA drives.
fn init_pata() {
	// TODO Add valid devices to disks list
	let dev0 = PATAInterface::new(false, false);
	if let Err(s) = dev0 {
		crate::println!("0: {}", s);
	} else {
		let d = dev0.unwrap();
		crate::println!("0: {} sectors {} {}", d.get_blocks_count(), d.is_atapi(), d.is_sata());
	}

	let dev1 = PATAInterface::new(false, true);
	if let Err(s) = dev1 {
		crate::println!("1: {}", s);
	} else {
		let d = dev1.unwrap();
		crate::println!("1: {} sectors {} {}", d.get_blocks_count(), d.is_atapi(), d.is_sata());
	}

	let dev2 = PATAInterface::new(true, false);
	if let Err(s) = dev2 {
		crate::println!("2: {}", s);
	} else {
		let d = dev2.unwrap();
		crate::println!("2: {} sectors {} {}", d.get_blocks_count(), d.is_atapi(), d.is_sata());
	}

	let dev3 = PATAInterface::new(true, true);
	if let Err(s) = dev3 {
		crate::println!("3: {}", s);
	} else {
		let d = dev3.unwrap();
		crate::println!("3: {} sectors {} {}", d.get_blocks_count(), d.is_atapi(), d.is_sata());
	}
}

/// Detects buses and devices.
pub fn detect() -> Result<(), Errno> {
	bus::detect()?;
	init_ps2();
	init_pata();

	// TODO

	Ok(())
}
