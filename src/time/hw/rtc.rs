//! The Real Time Clock (RTC) is the clock used by the CMOS to maintain system time.

use super::HwClock;
use crate::idt;
use crate::io;
use crate::util::math::rational::Rational;

/// The ID of the port used to select the CMOS register to read.
const SELECT_PORT: u16 = 0x70;
/// The ID of the port to read or write a CMOS port previously selected.
const VALUE_PORT: u16 = 0x71;

/// The ID of the status register A.
const STATUS_A_REGISTER: u8 = 0x0a;
/// The ID of the status register B.
const STATUS_B_REGISTER: u8 = 0x0b;
/// The ID of the status register C.
const STATUS_C_REGISTER: u8 = 0x0c;

// FIXME prevent having several instances at the same time

/// The RTC.
///
/// **Note**: the RTC needs a call to `reset` to allow the next tick to be fired.
pub struct RTC {}

impl RTC {
	/// Creates a new instance.
	///
	/// By default, the timer is disabled and its frequency is undefined.
	pub fn new() -> Self {
		let mut s = Self {};
		s.set_enabled(false);

		s
	}

	/// Resets the timer to make it ready for the next tick.
	#[inline]
	pub fn reset() {
		unsafe {
			io::outb(SELECT_PORT, STATUS_C_REGISTER);
			io::inb(VALUE_PORT);
		}
	}
}

impl HwClock for RTC {
	fn set_enabled(&mut self, enable: bool) {
		idt::wrap_disable_interrupts(|| unsafe {
			io::outb(SELECT_PORT, STATUS_B_REGISTER | 0x80);
			let prev = io::inb(VALUE_PORT);

			io::outb(SELECT_PORT, STATUS_B_REGISTER | 0x80);
			if enable {
				io::outb(VALUE_PORT, prev | 0x40);
			} else {
				io::outb(VALUE_PORT, prev & !0x40);
			}
		});
	}

	fn set_frequency(&mut self, _freq: Rational) {
		// TODO adapt to given frequency

		idt::wrap_disable_interrupts(|| unsafe {
			io::outb(0x70, STATUS_A_REGISTER | 0x80);
			let prev = io::inb(VALUE_PORT);
			io::outb(0x70, STATUS_A_REGISTER | 0x80);
			io::outb(0x71, (prev & 0xf0) | 6);
		});
	}

	fn get_interrupt_vector(&self) -> u32 {
		0x28
	}
}

impl Drop for RTC {
	fn drop(&mut self) {
		self.set_enabled(false);
	}
}
