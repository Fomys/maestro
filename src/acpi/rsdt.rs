//! This module handles ACPI's Root System Description Table (RSDT).

use super::ACPITable;
use super::ACPITableHeader;
use core::mem::size_of;

/// The Root System Description Table.
#[repr(C)]
#[derive(Debug)]
pub struct Rsdt {
	/// The table's header.
	pub header: ACPITableHeader,
}

// TODO XSDT

impl Rsdt {
	/// Iterates over every ACPI tables.
	pub fn foreach_table<F: FnMut(*const ACPITableHeader)>(&self, mut f: F) {
		let entries_len = self.header.get_length() - size_of::<Rsdt>();
		let entries_count = entries_len / 4;
		let entries_ptr = (self as *const _ as usize + size_of::<Rsdt>()) as *const u32;

		for i in 0..entries_count {
			let header_ptr = unsafe { *entries_ptr.add(i) as *const ACPITableHeader };

			f(header_ptr);
		}
	}
}

impl ACPITable for Rsdt {
	fn get_expected_signature() -> &'static [u8; 4] {
		&[b'R', b'S', b'D', b'T']
	}
}
