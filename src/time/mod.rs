//! This module handles time-releated features.
//! The kernel stores a list of clock sources. A clock source is an object that allow to get the
//! current timestamp.

pub mod timer;
pub mod unit;

use crate::errno::Errno;
use crate::util::boxed::Box;
use crate::util::container::vec::Vec;
use crate::util::lock::*;
use unit::TimeUnit;
use unit::Timestamp;
use unit::TimestampScale;

/// Trait representing a source able to provide the current timestamp.
pub trait ClockSource {
	/// The name of the source.
	fn get_name(&self) -> &str;
	/// Returns the current timestamp in seconds.
	/// `scale` specifies the scale of the returned timestamp.
	fn get_time(&mut self, scale: TimestampScale) -> Timestamp;
}

// TODO Order by name to allow binary search
/// Vector containing all the clock sources.
static CLOCK_SOURCES: Mutex<Vec<Box<dyn ClockSource>>> = Mutex::new(Vec::new());

/// Returns a reference to the list of clock sources.
pub fn get_clock_sources() -> &'static Mutex<Vec<Box<dyn ClockSource>>> {
	&CLOCK_SOURCES
}

/// Adds the new clock source to the clock sources list.
pub fn add_clock_source<T: 'static + ClockSource>(source: T) -> Result<(), Errno> {
	let guard = CLOCK_SOURCES.lock();
	let sources = guard.get_mut();
	sources.push(Box::new(source)?)?;
	Ok(())
}

/// Removes the clock source with the given name.
/// If the clock source doesn't exist, the function does nothing.
pub fn remove_clock_source(name: &str) {
	let guard = CLOCK_SOURCES.lock();
	let sources = guard.get_mut();

	for i in 0..sources.len() {
		if sources[i].get_name() == name {
			sources.remove(i);
			return;
		}
	}
}

/// Returns the current timestamp from the preferred clock source.
/// `scale` specifies the scale of the returned timestamp.
/// If no clock source is available, the function returns None.
pub fn get(scale: TimestampScale) -> Option<Timestamp> {
	let guard = CLOCK_SOURCES.lock();
	let sources = guard.get_mut();

	if !sources.is_empty() {
		let src = &mut sources[0]; // TODO Select the preferred source
		Some(src.get_time(scale))
	} else {
		None
	}
}

/// Returns the current timestamp from the given clock `clk`.
/// `scale` specifies the scale of the returned timestamp.
/// If the clock doesn't exist, the function returns None.
pub fn get_struct<T: TimeUnit>(_clk: &[u8]) -> Option<T> {
	// TODO use the given clock
	let ts = get(TimestampScale::Nanosecond)?;
	Some(T::from_nano(ts))
}

/// Makes the CPU wait for at least `n` milliseconds.
pub fn mdelay(n: u32) {
	// TODO
	udelay(n * 1000);
}

/// Makes the CPU wait for at least `n` microseconds.
pub fn udelay(n: u32) {
	// TODO
	for _ in 0..(n * 100) {
		unsafe {
			core::arch::asm!("nop");
		}
	}
}

/// Makes the CPU wait for at least `n` nanoseconds.
pub fn ndelay(n: u32) {
	// TODO
	udelay(n);
}
