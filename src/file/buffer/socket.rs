//! This file implements sockets.

use core::ffi::c_void;
use crate::errno::Errno;
use crate::file::buffer::BlockHandler;
use crate::process::mem_space::MemSpace;
use crate::syscall::ioctl;
use crate::util::FailableDefault;
use crate::util::io::IO;
use crate::util::ptr::IntSharedPtr;
use super::Buffer;

/// The maximum size of a socket's buffers.
const BUFFER_SIZE: usize = 65536;

/// Structure representing a socket.
#[derive(Debug)]
pub struct Socket {
	/// The socket's domain.
	domain: i32,
	/// The socket's type.
	type_: i32,
	/// The socket's protocol.
	protocol: i32,

	/// The socket's block handler.
	block_handler: BlockHandler,
}

impl Socket {
	/// Creates a new instance.
	pub fn new(domain: i32, type_: i32, protocol: i32) -> Self {
		// TODO Check domain, type and protocol. Use EINVAL, EPROTOTYPE and
		// EPROTONOSUPPORT

		Self {
			domain,
			type_,
			protocol,

			block_handler: BlockHandler::new(),
		}
	}

	/// Returns the socket's domain.
	#[inline(always)]
	pub fn get_domain(&self) -> i32 {
		self.domain
	}

	/// Returns the socket's type.
	#[inline(always)]
	pub fn get_type(&self) -> i32 {
		self.type_
	}

	/// Returns the socket's protocol.
	#[inline(always)]
	pub fn get_protocol(&self) -> i32 {
		self.protocol
	}
}

impl FailableDefault for Socket {
	fn failable_default() -> Result<Self, Errno> {
		// TODO Put correct params (unix domain)
		Ok(Self::new(0, 0, 0))
	}
}

impl Buffer for Socket {
	fn increment_open(&mut self, _read: bool, _write: bool) {
		// TODO
		todo!();
	}

	fn decrement_open(&mut self, _read: bool, _write: bool) {
		// TODO
		todo!();
	}

	fn get_block_handler(&mut self) -> &mut BlockHandler {
		&mut self.block_handler
	}

	fn ioctl(
		&mut self,
		_mem_space: IntSharedPtr<MemSpace>,
		_request: ioctl::Request,
		_argp: *const c_void,
	) -> Result<u32, Errno> {
		// TODO
		todo!();
	}
}

impl IO for Socket {
	fn get_size(&self) -> u64 {
		// TODO
		0
	}

	/// Note: This implemention ignores the offset.
	fn read(&mut self, _: u64, _buf: &mut [u8]) -> Result<(u64, bool), Errno> {
		// TODO
		todo!();
	}

	/// Note: This implemention ignores the offset.
	fn write(&mut self, _: u64, _buf: &[u8]) -> Result<u64, Errno> {
		// TODO
		todo!();
	}

	fn poll(&mut self, _mask: u32) -> Result<u32, Errno> {
		// TODO
		todo!();
	}
}
