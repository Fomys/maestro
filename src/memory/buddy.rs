/*
 * This module contains the buddy allocator which allows to allocate 2^^n pages
 * large frames of memory.
 *
 * This allocator works by dividing frames of memory in two until the a frame of
 * the required size is available.
 *
 * The order of a frame is the `n` in the expression `2^^n` that represents the
 * size of a frame in pages.
 */

use core::cmp::min;
use core::mem::MaybeUninit;
use crate::memory::NULL;
use crate::memory::Void;
use crate::memory::memmap;
use crate::memory;
use crate::util::lock::Mutex;
use crate::util::lock::MutexGuard;
use crate::util;

/*
 * Type representing the order of a memory frame.
 */
pub type FrameOrder = u8;
/*
 * Type representing buddy allocator flags.
 */
pub type Flags = i32;
/*
 * Type representing the identifier of a frame.
 */
type FrameID = u32;

/*
 * The maximum order of a buddy allocated frame.
 */
pub const MAX_ORDER: FrameOrder = 17;

/*
 * The mask for the type of the zone in buddy allocator flags.
 */
const ZONE_TYPE_MASK: Flags = 0b11;
/*
 * Buddy allocator flag. Tells that the allocated frame must be mapped into the user zone.
 */
pub const FLAG_ZONE_TYPE_USER: Flags = 0b000;
/*
 * Buddy allocator flag. Tells that the allocated frame must be mapped into the kernel zone.
 */
pub const FLAG_ZONE_TYPE_KERNEL: Flags = 0b001;
/*
 * Buddy allocator flag. Tells that the allocated frame must be mapped into the DMA zone.
 */
pub const FLAG_ZONE_TYPE_DMA: Flags = 0b010;
/*
 * Buddy allocator flag. Tells that the allocation shall not fail (unless not enough memory is
 * present on the system). This flag is ignored if FLAG_USER is not specified or if the allocation
 * order is higher than 0. The allocator shall use the OOM killer to recover memory.
 */
pub const FLAG_NOFAIL: Flags = 0b100;

/*
 * Pointer to the end of the kernel zone of memory with the maximum possible size.
 */
pub const KERNEL_ZONE_LIMIT: *const Void = 0x40000000 as _;

/*
 * Value indicating that the frame is used.
 */
pub const FRAME_STATE_USED: FrameID = !(0 as FrameID);

// TODO OOM killer

/*
 * Structure representing an allocatable zone of memory.
 */
struct Zone {
	/* The type of the zone, defining the priority */
	type_: Flags,
	/* The number of allocated pages in the zone */
	allocated_pages: usize,

	/* A pointer to the beginning of the metadata of the zone */
	metadata_begin: *mut Void,
	/* A pointer to the beginning of the allocatable memory of the zone */
	begin: *mut Void,
	/* The size of the zone in bytes */
	pages_count: FrameID,

	/* The free list containing linked lists to free frames */
	free_list: [Option<*mut Frame>; (MAX_ORDER + 1) as usize],
}

/*
 * Structure representing the metadata for a frame of physical memory. The structure has an
 * internal linked list for the free list. This linked list doesn't store pointers but frame
 * identifiers to save memory. If either `prev` or `next` has value `FRAME_STATE_USED`, the frame
 * is marked as used. If a frame points to itself, it means that no more elements are present in
 * the list.
 */
#[repr(packed)]
struct Frame {
	/* Identifier of the previous frame in the free list. */
	prev: FrameID,
	/* Identifier of the next frame in the free list. */
	next: FrameID,
	/* Order of the current frame */
	order: FrameOrder,
}

// TODO Remplace by a linked list? (in case of holes in memory)
/*
 * The array of buddy allocator zones.
 */
static mut ZONES: MaybeUninit<[Mutex<Zone>; 3]> = MaybeUninit::uninit();

/*
 * The size in bytes of a frame allocated by the buddy allocator with the given `order`.
 */
pub fn get_frame_size(order: FrameOrder) -> usize {
	memory::PAGE_SIZE << order
}

/*
 * Returns the buddy order required to fit the given number of pages.
 */
pub fn get_order(pages: usize) -> FrameOrder {
	let mut order: FrameOrder = 0;
	let mut i = 1;

	while i < pages {
		i *= 2;
		order += 1;
	}
	order
}

/*
 * Initializes the buddy allocator.
 */
pub fn init() {
	unsafe {
		util::zero_object(&mut ZONES);
	}

	let mmap_info = memmap::get_info();
	let z = unsafe {
		ZONES.assume_init_mut()
	};

	let virt_alloc_begin = memory::kern_to_virt(mmap_info.phys_alloc_begin);
	let metadata_begin = util::align(virt_alloc_begin, memory::PAGE_SIZE) as *mut Void;
	let frames_count = mmap_info.available_memory
		/ (memory::PAGE_SIZE + core::mem::size_of::<Frame>());
	let metadata_size = frames_count * core::mem::size_of::<Frame>();
	let metadata_end = unsafe { metadata_begin.add(metadata_size) };
	let phys_metadata_end = memory::kern_to_phys(metadata_end);
	// TODO Check that metadata doesn't exceed kernel space's capacity

	let kernel_zone_begin = util::align(phys_metadata_end, memory::PAGE_SIZE) as *mut Void;
	let kernel_zone_end = util::down_align(min(KERNEL_ZONE_LIMIT, mmap_info.phys_alloc_end),
		memory::PAGE_SIZE);
	let kernel_frames_count = (kernel_zone_end as usize - kernel_zone_begin as usize)
		/ memory::PAGE_SIZE;
	z[1].lock().get_mut().init(FLAG_ZONE_TYPE_KERNEL, metadata_begin, kernel_frames_count as _,
		kernel_zone_begin);
	z[1].unlock();

	// TODO
	z[0].lock().get_mut().init(FLAG_ZONE_TYPE_USER, 0 as *mut Void, 0, 0 as *mut Void);
	z[0].unlock();

	// TODO
	z[2].lock().get_mut().init(FLAG_ZONE_TYPE_DMA, 0 as *mut Void, 0, 0 as *mut Void);
	z[2].unlock();
}

// TODO Allow to fallback to another zone if the one that is returned is full
/*
 * Returns a mutable reference to a zone suitable for an allocation with the given type `type_`.
 */
fn get_suitable_zone(type_: Flags) -> Option<&'static mut Mutex<Zone>> {
	let zones = unsafe { ZONES.assume_init_mut() };

	for i in 0..zones.len() {
		let is_valid = {
			let guard = MutexGuard::new(&mut zones[i]);
			let zone = guard.get();
			zone.type_ == type_
		};
		if is_valid {
			return Some(&mut zones[i]);
		}
	}
	None
}

/*
 * Returns a mutable reference to the zone that contains the given pointer.
 */
fn get_zone_for_pointer(ptr: *const Void) -> Option<&'static mut Mutex<Zone>> {
	let zones = unsafe { ZONES.assume_init_mut() };

	for i in 0..zones.len() {
		let is_valid = {
			let guard = MutexGuard::new(&mut zones[i]);
			let zone = guard.get();
			ptr >= zone.begin && (ptr as usize) < (zone.begin as usize) + zone.get_size()
		};
		if is_valid {
			return Some(&mut zones[i]);
		}
	}
	None
}

/*
 * Allocates a frame of memory using the buddy allocator. `order` is the order of the frame to be
 * allocated.
 * TODO document flags
 */
pub fn alloc(order: FrameOrder, flags: Flags) -> Result<*mut Void, ()> {
	debug_assert!(order <= MAX_ORDER);

	let z = get_suitable_zone(flags & ZONE_TYPE_MASK);
	if let Some(z_) = z {
		let mut guard = MutexGuard::new(z_);
		let zone = guard.get_mut();

		let frame = zone.get_available_frame(order);
		if let Some(f) = frame {
			f.split(zone, order);
			f.mark_used();
			zone.allocated_pages += util::pow2(order as _) as usize;

			let ptr = f.get_ptr(zone);
			debug_assert!(util::is_aligned(ptr, memory::PAGE_SIZE));
			debug_assert!(ptr >= zone.begin && ptr < (zone.begin as usize + zone.get_size()) as _);
			return Ok(ptr);
		}
	}
	Err(())
}

/*
 * Calls `alloc` with order `order`. The allocated frame is in the kernel zone.
 * The function returns the *virtual* address, not the physical one.
 */
pub fn alloc_kernel(order: FrameOrder) -> Result<*mut Void, ()> {
	Ok(memory::kern_to_virt(alloc(order, FLAG_ZONE_TYPE_KERNEL)?) as _)
}

/*
 * Frees the given memory frame that was allocated using the buddy allocator. The given order must
 * be the same as the one given to allocate the frame.
 */
pub fn free(ptr: *const Void, order: FrameOrder) {
	debug_assert!(util::is_aligned(ptr, memory::PAGE_SIZE));
	debug_assert!(order <= MAX_ORDER);

	let z = get_zone_for_pointer(ptr);
	if let Some(z_) = z {
		let mut guard = MutexGuard::new(z_);
		let zone = guard.get_mut();

		let frame_id = zone.get_frame_id_from_ptr(ptr);
		debug_assert!(frame_id < zone.get_pages_count());
		let frame = zone.get_frame(frame_id);
		unsafe {
			(*frame).mark_free();
			(*frame).coalesce(zone);
		}
		zone.allocated_pages -= util::pow2(order as _) as usize;
	}
}

/*
 * Frees the given memory frame. `ptr` is the *virtual* address to the beginning of the frame and
 * and `order` is the order of the frame.
 */
pub fn free_kernel(ptr: *const Void, order: FrameOrder) {
	free(memory::kern_to_phys(ptr), order);
}

/*
 * Returns the total number of pages allocated by the buddy allocator.
 */
pub fn allocated_pages() -> usize {
	let mut n = 0;

	unsafe {
		let z = ZONES.assume_init_mut();
		for i in 0..z.len() {
			let guard = MutexGuard::new(&mut z[i]); // TODO Remove `mut`?
			n += guard.get().get_allocated_pages();
		}
	}
	n
}

impl Zone {
	/*
	 * Fills the free list during initialization according to the number of available pages.
	 */
	fn fill_free_list(&mut self) {
		let pages_count = self.get_pages_count();
		let mut frame: FrameID = 0;
		let mut order = MAX_ORDER;

		while frame < pages_count as FrameID {
			let p = util::pow2(order as _) as FrameID;
			if frame + p > pages_count {
				if order == 0 {
					break;
				}
				order -= 1;
				continue;
			}

			let f = unsafe { &mut *self.get_frame(frame) };
			f.mark_free();
			f.order = order;
			f.link(self);

			frame += p;
		}
	}

	/*
	 * Initializes the zone with type `type_`. The zone covers the memory from pointer `begin` to
	 * `begin + size` where `size` is the size in bytes.
	 */
	pub fn init(&mut self, type_: Flags, metadata_begin: *mut Void, pages_count: FrameID,
		begin: *mut Void) {
		self.type_ = type_;
		self.allocated_pages = 0;
		self.metadata_begin = metadata_begin;
		self.begin = begin;
		self.pages_count = pages_count;
		self.fill_free_list();
	}

	/*
	 * Returns the number of allocated pages in the current zone of memory.
	 */
	pub fn get_allocated_pages(&self) -> usize {
		self.allocated_pages
	}

	/*
	 * Returns the number of allocatable pages.
	 */
	pub fn get_pages_count(&self) -> FrameID {
		self.pages_count
	}

	/*
	 * Returns the size in bytes of the allocatable memory.
	 */
	pub fn get_size(&self) -> usize {
		(self.pages_count as usize) * memory::PAGE_SIZE
	}

	/*
	 * Returns an available frame owned by this zone, with an order of at least `order`.
	 */
	pub fn get_available_frame(&self, order: FrameOrder) -> Option<&'static mut Frame> {
		for i in (order as usize)..self.free_list.len() {
			let f = self.free_list[i];
			if let Some(f_) = f {
				return Some(unsafe { &mut *f_ });
			}
		}
		None
	}

	/*
	 * Returns the identifier for the frame at the given pointer `ptr`. The pointer must point to
	 * the frame itself, not the Frame structure.
	 */
	pub fn get_frame_id_from_ptr(&self, ptr: *const Void) -> FrameID {
		(((ptr as usize) - (self.begin as usize)) / memory::PAGE_SIZE) as _
	}

	/*
	 * Returns a mutable reference to the frame with the given identifier `id`.
	 * The given identifier **must** be in the range of the zone.
	 */
	pub fn get_frame(&self, id: FrameID) -> *mut Frame {
		debug_assert!(id < self.get_pages_count());
		let off = (self.metadata_begin as usize) + (id as usize * core::mem::size_of::<Frame>());
		off as _
	}

	/*
	 * Debug function.
	 * Checks the correctness of the free list for the zone. Every frames in the free list must
	 * have an order lower or equal the max order and must be free.
	 * If a frame is the first of a list, it must not have a previous element.
	 *
	 * The function returns `true` if the free list is correct, or `false if not.
	 */
	#[cfg(kernel_mode = "debug")]
	pub fn check_free_list(&self) -> bool {
		for (_order, list) in self.free_list.iter().enumerate() {
			if let Some(first) = *list {
				let mut frame = first;
				let mut is_first = true;

				loop {
					let f = unsafe { &*frame };
					let id = f.get_id(self);

					if f.is_used() {
						return false;
					}
					if f.order > MAX_ORDER {
						return false;
					}
					if is_first && f.prev != id {
						return false;
					}

					if f.next == id {
						break;
					}
					frame = self.get_frame(f.next);
					is_first = false;
				}
			}
		}
		true
	}
}

impl Frame {
	/*
	 * Returns the id of the current frame in the associated zone `zone`.
	 */
	pub fn get_id(&self, zone: &Zone) -> FrameID {
		let self_off = self as *const _ as usize;
		let zone_off = zone.metadata_begin as *const _ as usize;
		debug_assert!(self_off >= zone_off);
		((self_off - zone_off) / core::mem::size_of::<Self>()) as u32
	}

	/*
	 * Returns the pointer to the location of the associated physical memory.
	 */
	pub fn get_ptr(&self, zone: &Zone) -> *mut Void {
		let off = self.get_id(zone) as usize * memory::PAGE_SIZE;
		(zone.begin as usize + off) as _
	}

	/*
	 * Tells whether the frame is used or not.
	 */
	pub fn is_used(&self) -> bool {
		(self.prev == FRAME_STATE_USED) || (self.next == FRAME_STATE_USED)
	}

	/*
	 * Marks the frame as used. The frame must not be linked to any free list.
	 */
	pub fn mark_used(&mut self) {
		self.prev = FRAME_STATE_USED;
		self.next = FRAME_STATE_USED;
	}

	/*
	 * Marks the frame as free. The frame must not be linked to any free list.
	 */
	pub fn mark_free(&mut self) {
		self.prev = 0;
		self.next = 0;
	}

	/*
	 * Returns the identifier of the buddy frame in zone `zone`, taking in account the frame's
	 * order.
	 * The return value might be invalid and the caller has the reponsibility to check that it is
	 * below the number of frames in the zone.
	 */
	pub fn get_buddy_id(&self, zone: &Zone) -> FrameID {
		self.get_id(zone) ^ (1 << self.order) as u32
	}

	/*
	 * Links the frame into zone `zone`'s free list.
	 */
	pub fn link(&mut self, zone: &mut Zone) {
		debug_assert!(!self.is_used());
		debug_assert!(zone.check_free_list());

		let id = self.get_id(zone);
		self.prev = id;
		self.next = if let Some(n) = zone.free_list[self.order as usize] {
			let next = unsafe { &mut *n };
			debug_assert!(!next.is_used());
			next.prev = id;
			next.get_id(zone)
		} else {
			id
		};
		zone.free_list[self.order as usize] = Some(self);

		debug_assert!(zone.check_free_list());
	}

	/*
	 * Unlinks the frame from zone `zone`'s free list.
	 */
	pub fn unlink(&mut self, zone: &mut Zone) {
		debug_assert!(!self.is_used());
		debug_assert!(zone.check_free_list());

		let id = self.get_id(zone);
		let has_prev = self.prev != id;
		let has_next = self.next != id;
		if has_prev {
			let prev = zone.get_frame(self.prev);
			unsafe {
				(*prev).next = if has_next { self.next } else { self.prev };
			}
		} else {
			zone.free_list[self.order as usize] = if has_next {
				Some(zone.get_frame(self.next))
			} else {
				None
			}
		}

		if has_next {
			let next = zone.get_frame(self.next);
			unsafe {
				(*next).prev = if has_prev { self.prev } else { self.next };
			}
		}

		debug_assert!(zone.check_free_list());
	}

	/*
	 * Unlinks the frame from zone `zone`'s free list, splits it until it reaches the required
	 * order `order` while linking the new free frames to the free list. At the end of the
	 * function, the current frame is **not** linked to the free list.
	 *
	 * The frame must not be marked as used.
	 */
	pub fn split(&mut self, zone: &mut Zone, order: FrameOrder) {
		debug_assert!(!self.is_used());
		debug_assert!(self.order >= order);

		self.unlink(zone);
		while self.order > order {
			self.order -= 1;

			let buddy = self.get_buddy_id(zone);
			debug_assert!(buddy != self.get_id(zone));
			if buddy >= zone.get_pages_count() {
				break;
			}

			let buddy_frame = unsafe { &mut *zone.get_frame(buddy) };
			debug_assert!(!buddy_frame.is_used());
			buddy_frame.unlink(zone);
			buddy_frame.order = self.order;
			buddy_frame.link(zone);
		}
	}

	/*
	 * Coealesces the frame in zone `zone` with free buddy blocks recursively until no buddy is
	 * available anymore. Buddies that are merges with the frame are unlinked. The order of the
	 * frame is incremented at each merge. The frame is linked to the free list at the end.
	 *
	 * The frame must not be marked as used.
	 */
	pub fn coalesce(&mut self, zone: &mut Zone) {
		debug_assert!(!self.is_used());

		while self.order < MAX_ORDER {
			let buddy = self.get_buddy_id(zone);
			if buddy >= zone.get_pages_count() {
				break;
			}

			let buddy_frame = unsafe { &mut *zone.get_frame(buddy) };
			if buddy_frame.order != self.order || buddy_frame.is_used() {
				break;
			}

			buddy_frame.unlink(zone);
			self.order += 1;
		}
		self.link(zone);
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test_case]
	fn buddy0() {
		if let Ok(p) = alloc_kernel(0) {
			unsafe {
				util::memset(p, -1, get_frame_size(0));
			}
			free_kernel(p, 0);
		} else {
			assert!(false);
		}
	}

	#[test_case]
	fn buddy1() {
		if let Ok(p) = alloc_kernel(1) {
			unsafe {
				util::memset(p, -1, get_frame_size(1));
			}
			free_kernel(p, 1);
		} else {
			assert!(false);
		}
	}

	fn lifo_test(i: usize) {
		if let Ok(p) = alloc_kernel(0) {
			unsafe {
				util::memset(p, -1, get_frame_size(0));
			}
			if i > 0 {
				lifo_test(i - 1);
			}
			free_kernel(p, 0);
		} else {
			assert!(false);
		}
	}

	#[test_case]
	fn buddy_lifo() {
		lifo_test(100);
	}

	#[test_case]
	fn buddy_fifo() {
		let mut frames: [*const Void; 100] = [NULL; 100];

		for i in 0..frames.len() {
			if let Ok(p) = alloc_kernel(0) {
				frames[i] = p;
			} else {
				assert!(false);
			}
		}

		for i in 0..frames.len() {
			free_kernel(frames[i], 0);
		}
	}

	fn get_dangling(order: FrameOrder) -> *mut Void {
		if let Ok(p) = alloc_kernel(order) {
			unsafe {
				util::memset(p, -1, get_frame_size(order));
			}
			free_kernel(p, 0);
			p
		} else {
			assert!(false);
			memory::NULL as _
		}
	}

	#[test_case]
	fn buddy_free() {
		let first = get_dangling(0);
		for _ in 0..100 {
			assert_eq!(get_dangling(0), first);
		}
	}

	struct TestDupNode {
		next: *mut TestDupNode,
	}

	fn has_cycle(begin: *const TestDupNode) -> bool {
		if begin != NULL as _ {
			return false;
		}

		let mut tortoise = begin;
		let mut hoare = unsafe { (*begin).next };
		while (tortoise != NULL as _) && (hoare != NULL as _) && (tortoise != hoare) {
			tortoise = unsafe { (*tortoise).next };

			if unsafe { (*hoare).next } != NULL as _ {
				return false;
			}
			hoare = unsafe { (*(*hoare).next).next };
		}
		tortoise == hoare
	}

	#[test_case]
	fn buddy_full_duplicate() {
		let mut first = NULL as *mut TestDupNode;
		while let Ok(p) = alloc_kernel(0) {
			let node = p as *mut TestDupNode;
			unsafe {
				(*node).next = first;
			}
			first = node;
			assert!(!has_cycle(first));
		}

		while first != NULL as _ {
			let next = unsafe { (*first).next };
			free_kernel(first as _, 0);
			first = next;
		}
	}

	// TODO Add more tests
}
