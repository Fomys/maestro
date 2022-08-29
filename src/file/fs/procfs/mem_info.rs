//! This module implements the meminfo node, allowing to retrieve informations about memory usage
//! of the system.

use crate::errno::Errno;
use crate::file::fs::kernfs::node::KernFSNode;
use crate::file::FileContent;
use crate::file::Mode;
use crate::memory;
use crate::util::io::IO;
use crate::util::ptr::cow::Cow;
use core::cmp::min;

/// Structure representing the meminfo node.
pub struct MemInfo {}

impl KernFSNode for MemInfo {
	fn get_mode(&self) -> Mode {
		0o444
	}

	fn get_content<'a>(&'a self) -> Cow<'a, FileContent> {
		FileContent::Regular.into()
	}
}

impl IO for MemInfo {
	fn get_size(&self) -> u64 {
		0
	}

	fn read(&mut self, offset: u64, buff: &mut [u8]) -> Result<(u64, bool), Errno> {
		if buff.is_empty() {
			return Ok((0, false));
		}

		// Generating content
		let mem_info_guard = memory::stats::MEM_INFO.lock();
		let mem_info = mem_info_guard.get();
		let content = mem_info.to_string()?;

		// Copying content to userspace buffer
		let content_bytes = content.as_bytes();
		let len = min((content_bytes.len() as u64 - offset) as usize, buff.len());
		buff[..len].copy_from_slice(&content_bytes[(offset as usize)..(offset as usize + len)]);

		let eof = (offset + len as u64) >= content_bytes.len() as u64;
		Ok((len as _, eof))
	}

	fn write(&mut self, _offset: u64, _buff: &[u8]) -> Result<u64, Errno> {
		Err(errno!(EINVAL))
	}

	fn poll(&mut self, _mask: u32) -> Result<u32, Errno> {
		// TODO
		todo!();
	}
}
