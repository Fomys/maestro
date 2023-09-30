//! The pipe system call allows to create a pipe.

use crate::file::open_file::OpenFile;
use crate::errno::Errno;
use crate::file::buffer;
use crate::file::buffer::pipe::PipeBuffer;
use crate::file::open_file;
use crate::process::mem_space::ptr::SyscallPtr;
use crate::process::Process;
use crate::util::lock::Mutex;
use crate::util::ptr::arc::Arc;
use crate::util::TryDefault;
use core::ffi::c_int;
use macros::syscall;

#[syscall]
pub fn pipe(pipefd: SyscallPtr<[c_int; 2]>) -> Result<i32, Errno> {
	let proc_mutex = Process::current_assert();
	let proc = proc_mutex.lock();

	let mem_space = proc.get_mem_space().unwrap();
	let mut mem_space_guard = mem_space.lock();
	let pipefd_slice = pipefd
		.get_mut(&mut mem_space_guard)?
		.ok_or(errno!(EFAULT))?;

	let loc = buffer::register(None, Arc::new(Mutex::new(PipeBuffer::try_default()?))?)?;

	let fds_mutex = proc.get_fds().unwrap();
	let mut fds = fds_mutex.lock();

	let open_file0 = OpenFile::new(loc.clone(), open_file::O_RDONLY)?;
	let fd0 = fds.create_fd(0, open_file0)?;
	pipefd_slice[0] = fd0.get_id() as _;

	let open_file1 = OpenFile::new(loc, open_file::O_WRONLY)?;
	let fd1 = fds.create_fd(0, open_file1)?;
	pipefd_slice[1] = fd1.get_id() as _;

	Ok(0)
}
