//! This module implements the network stack.

pub mod icmp;
pub mod ip;
pub mod lo;
pub mod netlink;
pub mod osi;
pub mod sockaddr;
pub mod tcp;

use core::cmp::Ordering;
use core::ptr::NonNull;
use core::ptr;
use crate::errno::Errno;
use crate::util::boxed::Box;
use crate::util::container::string::String;
use crate::util::container::vec::Vec;
use crate::util::lock::Mutex;
use crate::util::ptr::SharedPtr;

/// Type representing a Media Access Control (MAC) address.
pub type MAC = [u8; 6];

/// An enumeration of network address types.
#[derive(Debug, Eq, PartialEq)]
pub enum Address {
	/// Internet Protocol version 4.
	IPv4([u8; 4]),
	/// Internet Protocol version 6.
	IPv6([u8; 16]),
}

/// An address/subnet mask pair to be bound to an interface.
#[derive(Debug)]
pub struct BindAddress {
	/// The bound address.
	pub addr: Address,
	/// Subnet mask/prefix length.
	pub subnet_mask: u8,
}

impl BindAddress {
	/// Tells whether the bind address is suitable for transmission to the given destination
	/// address.
	pub fn is_matching(&self, addr: &Address) -> bool {
		fn check<const N: usize>(a: &[u8; N], b: &[u8; N], mask: usize) -> bool {
			a.array_chunks::<4>()
				.zip(b.array_chunks::<4>())
				.enumerate()
				.all(|(i, (a, b))| {
					let a = u32::from_ne_bytes(*a);
					let b = u32::from_ne_bytes(*b);

					let order = 32 - mask.checked_sub(i * 32).unwrap_or(0);
					let mask = !((1 << order) - 1);

					(a & mask) == (b & mask)
				})
		}

		match (&self.addr, addr) {
			(Address::IPv4(a), Address::IPv4(b)) => check(a, b, self.subnet_mask as _),
			(Address::IPv6(a), Address::IPv6(b)) => check(a, b, self.subnet_mask as _),

			_ => false,
		}
	}
}

/// Trait representing a network interface.
pub trait Interface {
	/// Returns the name of the interface.
	fn get_name(&self) -> &[u8];

	/// Tells whether the interface is UP.
	fn is_up(&self) -> bool;

	/// Returns the mac address of the interface.
	fn get_mac(&self) -> &MAC;

	/// Returns the list of addresses bound to the interface.
	fn get_addresses(&self) -> &[BindAddress];

	/// Reads data from the network interface and writes it into `buff`.
	///
	/// The function returns the number of bytes read.
	fn read(&mut self, buff: &mut [u8]) -> Result<u64, Errno>;

	/// Reads data from `buff` and writes it into the network interface.
	///
	/// The function returns the number of bytes written.
	fn write(&mut self, buff: &[u8]) -> Result<u64, Errno>;
}

/// An entry in the routing table.
pub struct Route {
	/// The destination address. If `None`, this is the default destination.
	dst: Option<BindAddress>,

	/// The name of the network interface.
	iface: String,
	/// The gateway's address.
	gateway: Address,

	/// The route's metric. The route with the lowest metric has priority.
	metric: u32,
}

impl Route {
	/// Tells whether the route matches the given address.
	pub fn is_matching(&self, addr: &Address) -> bool {
		// Check gateway
		if &self.gateway == addr {
			return true;
		}

		let Some(ref dst) = self.dst else {
			// Default route
			return true;
		};

		// Check with netmask
		dst.is_matching(addr)
	}

	/// Compares the current route with the given route `other`.
	///
	/// Ordering is done so that the best route is the greatest.
	pub fn cmp_for(&self, other: &Self, addr: &Address) -> Ordering {
		// Check gateway
		let self_match = addr == &self.gateway;
		let other_match = addr == &other.gateway;

		self_match.cmp(&other_match)
			.then_with(|| {
				// Check for matching network prefix

				let self_match = self.dst
					.as_ref()
					.map(|dst| dst.is_matching(addr))
					// Default address
					.unwrap_or(true);

				let other_match = other.dst
					.as_ref()
					.map(|dst| dst.is_matching(addr))
					// Default address
					.unwrap_or(true);

				self_match.cmp(&other_match)
			})
			.then_with(|| {
				// Check metric
				self.metric.cmp(&other.metric)
			})
	}
}

/// The list of network interfaces.
pub static INTERFACES: Mutex<Vec<Box<dyn Interface>>> = Mutex::new(Vec::new());
/// The routing table.
pub static ROUTING_TABLE: Mutex<Vec<Route>> = Mutex::new(Vec::new());

/// Registers the given network interface.
pub fn register_iface<I: 'static + Interface>(iface: I) -> Result<(), Errno> {
	let mut interfaces = INTERFACES.lock();

	let i = Box::new(iface)?;
	interfaces.push(i)
}

/// Unregisters the network interface with the given name.
pub fn unregister_iface(_name: &[u8]) {
	// TODO
	todo!();
}

/// Returns the network interface with the given name.
///
/// If the interface doesn't exist, thhe function returns `None`.
pub fn get_iface(_name: &[u8]) -> Option<SharedPtr<dyn Interface>> {
	// TODO
	todo!();
}

/// Returns the network interface to be used to transmit a packet to the given destination address.
pub fn get_iface_for(addr: Address) -> Option<SharedPtr<dyn Interface>> {
	let routing_table = ROUTING_TABLE.lock();
	let route = routing_table
		.iter()
		.filter(|route| route.is_matching(&addr))
		.max_by(|a, b| a.cmp_for(&b, &addr))?;

	get_iface(&route.iface)
}

/// A linked-list of buffers representing a packet being built.
pub struct BuffList<'b> {
	/// The buffer.
	b: &'b [u8],

	/// The next buffer in the list.
	next: Option<NonNull<BuffList<'b>>>,
	/// The length of following buffers.
	next_len: usize,
}

impl<'b> From<&'b [u8]> for BuffList<'b> {
	fn from(b: &'b [u8]) -> Self {
		Self {
			b,

			next: None,
			next_len: 0,
		}
	}
}

impl<'b> BuffList<'b> {
	/// Returns the length of the buffer, plus following buffers.
	pub fn len(&self) -> usize {
		self.b.len() + self.next_len
	}

	/// Pushes another buffer at the front of the list.
	pub fn push_front<'o>(&mut self, mut other: BuffList<'o>) -> BuffList<'o> where 'b: 'o {
		other.next = NonNull::new(self);
		other.next_len = self.b.len() + self.next_len;

		other
	}

	/// Collects all buffers into one.
	pub fn collect(&self) -> Result<Vec<u8>, Errno> {
		let len = self.len();
		let mut final_buff = crate::vec![0; len]?;

		let mut node = NonNull::new(self as *const _ as *mut Self);
		let mut i = 0;
		while let Some(mut curr) = node {
			let curr = unsafe {
				curr.as_mut()
			};
			let buf = curr.b;
			unsafe {
				ptr::copy_nonoverlapping(buf.as_ptr(), &mut final_buff[i], buf.len());
			}

			node = curr.next;
			i += buf.len();
		}

		Ok(final_buff)
	}
}

/// A network layer. Such objects can be stacked to for the network stack.
///
/// A layer stack acts as a pipeline, passing packets from one layer to the other.
pub trait Layer {
	// TODO receive

	/// Transmits data in the given buffer.
	///
	/// Arguments:
	/// - `buff` is the list of buffer which composes the packet being built.
	/// - `next` is the function called to pass the buffers list to the next layer.
	fn transmit<'c, F>(
		&self,
		buff: BuffList<'c>,
		next: F
	) -> Result<(), Errno>
		where Self: Sized, F: Fn(BuffList<'c>) -> Result<(), Errno>;
}
