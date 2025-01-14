//! A hashmap is a data structure that stores key/value pairs into buckets and
//! uses the hash of the key to quickly get the bucket storing the value.

use super::vec::Vec;
use crate::errno::AllocResult;
use crate::util::AllocError;
use crate::util::TryClone;
use core::borrow::Borrow;
use core::fmt;
use core::hash::Hash;
use core::hash::Hasher;
use core::iter::FusedIterator;
use core::iter::TrustedLen;
use core::mem::size_of_val;
use core::ops::Index;
use core::ops::IndexMut;

/// The default number of buckets in a hashmap.
const DEFAULT_BUCKETS_COUNT: usize = 64;

/// Bitwise XOR hasher.
struct XORHasher {
	/// The currently stored value.
	value: u64,
	/// The offset byte at which the next XOR operation shall be performed.
	off: u8,
}

impl XORHasher {
	/// Creates a new instance.
	pub fn new() -> Self {
		Self {
			value: 0,
			off: 0,
		}
	}
}

impl Hasher for XORHasher {
	fn write(&mut self, bytes: &[u8]) {
		for b in bytes {
			self.value ^= (*b as u64) << (self.off * 8);
			self.off = (self.off + 1) % size_of_val(&self.value) as u8;
		}
	}

	fn finish(&self) -> u64 {
		self.value
	}
}

/// A bucket is a list storing elements that match a given hash range.
///
/// Since hashing function have collisions, several elements can have the same
/// hash.
#[derive(Debug)]
struct Bucket<K: Eq + Hash, V> {
	/// The vector storing the key/value pairs.
	elements: Vec<(K, V)>,
}

impl<K: Eq + Hash, V> Bucket<K, V> {
	/// Creates a new instance.
	fn new() -> Self {
		Self {
			elements: Vec::new(),
		}
	}

	/// Returns an immutable reference to the value with the given key `k`.
	///
	/// If the key isn't present, the function return `None`.
	pub fn get<Q: ?Sized>(&self, k: &Q) -> Option<&V>
	where
		K: Borrow<Q>,
		Q: Hash + Eq,
	{
		for i in 0..self.elements.len() {
			if self.elements[i].0.borrow() == k {
				return Some(&self.elements[i].1);
			}
		}

		None
	}

	/// Returns a mutable reference to the value with the given key `k`.
	///
	/// If the key isn't present, the function return `None`.
	pub fn get_mut<Q: ?Sized>(&mut self, k: &Q) -> Option<&mut V>
	where
		K: Borrow<Q>,
		Q: Hash + Eq,
	{
		for i in 0..self.elements.len() {
			if self.elements[i].0.borrow() == k {
				return Some(&mut self.elements[i].1);
			}
		}

		None
	}

	/// Inserts a new element into the bucket.
	///
	/// If the key was already present, the function returns the previous value.
	pub fn insert(&mut self, k: K, v: V) -> AllocResult<Option<V>> {
		let old = self.remove(&k);
		self.elements.push((k, v))?;
		Ok(old)
	}

	/// Removes an element from the bucket.
	///
	/// If the key was present, the function returns the value.
	pub fn remove<Q: ?Sized>(&mut self, k: &Q) -> Option<V>
	where
		K: Borrow<Q>,
		Q: Hash + Eq,
	{
		for i in 0..self.elements.len() {
			if self.elements[i].0.borrow() == k {
				return Some(self.elements.remove(i).1);
			}
		}

		None
	}
}

impl<K: Eq + Hash + TryClone<Error = E>, V: TryClone<Error = E>, E: From<AllocError>> TryClone
	for Bucket<K, V>
{
	type Error = E;

	fn try_clone(&self) -> Result<Self, Self::Error> {
		let mut v = Vec::with_capacity(self.elements.len())?;
		for (key, value) in self.elements.iter() {
			v.push((key.try_clone()?, value.try_clone()?))?;
		}

		Ok(Self {
			elements: v,
		})
	}
}

/// Structure representing a hashmap.
#[derive(Debug)]
pub struct HashMap<K: Eq + Hash, V> {
	/// The number of buckets in the hashmap.
	buckets_count: usize,
	/// The vector containing buckets.
	buckets: Vec<Bucket<K, V>>,

	/// The number of elements in the container.
	len: usize,
}

impl<K: Eq + Hash, V> Default for HashMap<K, V> {
	fn default() -> Self {
		Self::new()
	}
}

impl<K: Eq + Hash, V, const N: usize> TryFrom<[(K, V); N]> for HashMap<K, V> {
	type Error = AllocError;

	fn try_from(arr: [(K, V); N]) -> Result<Self, Self::Error> {
		let mut h = HashMap::new();
		for (key, value) in arr {
			h.insert(key, value)?;
		}

		Ok(h)
	}
}

impl<K: Eq + Hash, V> HashMap<K, V> {
	/// Creates a new instance with the default number of buckets.
	pub const fn new() -> Self {
		Self {
			buckets_count: DEFAULT_BUCKETS_COUNT,
			buckets: Vec::new(),

			len: 0,
		}
	}

	/// Creates a new instance with the given number of buckets.
	pub const fn with_buckets(buckets_count: usize) -> Self {
		Self {
			buckets_count,
			buckets: Vec::new(),

			len: 0,
		}
	}

	/// Returns the number of elements in the hash map.
	#[inline]
	pub fn len(&self) -> usize {
		self.len
	}

	/// Tells whether the hash map is empty.
	#[inline]
	pub fn is_empty(&self) -> bool {
		self.len == 0
	}

	/// Returns the number of buckets.
	#[inline]
	pub fn get_buckets_count(&self) -> usize {
		self.buckets_count
	}

	/// Returns the bucket index for the key `k`.
	fn get_bucket_index<Q: ?Sized>(&self, k: &Q) -> usize
	where
		K: Borrow<Q>,
		Q: Hash,
	{
		let mut hasher = XORHasher::new();
		k.hash(&mut hasher);
		(hasher.finish() % (self.buckets_count as u64)) as usize
	}

	/// Returns an immutable reference to the value with the given key `k`.
	///
	/// If the key isn't present, the function return `None`.
	pub fn get<Q: ?Sized>(&self, k: &Q) -> Option<&V>
	where
		K: Borrow<Q>,
		Q: Hash + Eq,
	{
		let index = self.get_bucket_index(k);

		if index < self.buckets.len() {
			self.buckets[index].get(k)
		} else {
			None
		}
	}

	/// Returns a mutable reference to the value with the given key `k`.
	///
	/// If the key isn't present, the function return `None`.
	pub fn get_mut<Q: ?Sized>(&mut self, k: &Q) -> Option<&mut V>
	where
		K: Borrow<Q>,
		Q: Hash + Eq,
	{
		let index = self.get_bucket_index(k);

		if index < self.buckets.len() {
			self.buckets[index].get_mut(k)
		} else {
			None
		}
	}

	/// Tells whether the hash map contains the given key `k`.
	#[inline]
	pub fn contains_key<Q: ?Sized>(&self, k: &Q) -> bool
	where
		K: Borrow<Q>,
		Q: Hash + Eq,
	{
		self.get(k).is_some()
	}

	/// Creates an iterator of immutable references for the hash map.
	#[inline]
	pub fn iter(&self) -> Iter<K, V> {
		Iter {
			hm: self,

			curr_bucket: 0,
			curr_element: 0,
			i: 0,
		}
	}

	/// Inserts a new element into the hash map.
	///
	/// If the key was already present, the function returns the previous value.
	pub fn insert(&mut self, k: K, v: V) -> AllocResult<Option<V>> {
		let index = self.get_bucket_index(&k);
		if index >= self.buckets.len() {
			// Creating buckets
			let begin = self.buckets.len();
			for i in begin..=index {
				self.buckets.insert(i, Bucket::new())?;
			}
		}

		let result = self.buckets[index].insert(k, v)?;

		if result.is_none() {
			self.len += 1;
		}

		Ok(result)
	}

	/// Removes an element from the hash map.
	///
	/// If the key was present, the function returns the previous value.
	pub fn remove<Q: ?Sized>(&mut self, k: &Q) -> Option<V>
	where
		K: Borrow<Q>,
		Q: Hash + Eq,
	{
		let index = self.get_bucket_index(k);

		if index < self.buckets.len() {
			let result = self.buckets[index].remove(k);

			if result.is_some() {
				self.len -= 1;
			}

			result
		} else {
			None
		}
	}

	/// Retains only the elements for which the given predicate returns `true`.
	pub fn retain<F: FnMut(&K, &mut V) -> bool>(&mut self, mut f: F) {
		let mut len = 0;

		for b in self.buckets.iter_mut() {
			b.elements.retain(|(k, v): &mut (K, V)| f(k, &mut *v));
			len += b.elements.len();
		}

		self.len = len;
	}

	/// Drops all elements in the hash map.
	pub fn clear(&mut self) {
		for i in 0..self.buckets.len() {
			self.buckets[i].elements.clear();
		}

		self.len = 0;
	}
}

impl<K: Eq + Hash, V> Index<K> for HashMap<K, V> {
	type Output = V;

	#[inline]
	fn index(&self, k: K) -> &Self::Output {
		self.get(&k).expect("no entry found for key")
	}
}

impl<K: Eq + Hash, V> IndexMut<K> for HashMap<K, V> {
	#[inline]
	fn index_mut(&mut self, k: K) -> &mut Self::Output {
		self.get_mut(&k).expect("no entry found for key")
	}
}

impl<K: Eq + Hash + TryClone<Error = E>, V: TryClone<Error = E>, E: From<AllocError>> TryClone
	for HashMap<K, V>
{
	type Error = E;

	fn try_clone(&self) -> Result<Self, Self::Error> {
		Ok(Self {
			buckets_count: self.buckets_count,
			buckets: self.buckets.try_clone()?,

			len: self.len,
		})
	}
}

/// Iterator for the [`HashMap`] structure.
///
/// This iterator doesn't guarantee any order since the HashMap itself doesn't store value in a
/// specific order.
pub struct Iter<'m, K: Hash + Eq, V> {
	/// The hash map to iterate into.
	hm: &'m HashMap<K, V>,

	/// The current bucket index.
	curr_bucket: usize,
	/// The current element index.
	curr_element: usize,
	/// Number of elements iterated on so far
	i: usize,
}

impl<'m, K: Hash + Eq, V> Iterator for Iter<'m, K, V> {
	type Item = (&'m K, &'m V);

	fn next(&mut self) -> Option<Self::Item> {
		if self.curr_bucket >= self.hm.buckets.len() {
			return None;
		}

		// If the last element has been reached, getting the next non-empty bucket
		if self.curr_element >= self.hm.buckets[self.curr_bucket].elements.len() {
			self.curr_element = 0;
			self.curr_bucket += 1;

			for i in self.curr_bucket..self.hm.buckets.len() {
				if !self.hm.buckets[i].elements.is_empty() {
					break;
				}

				self.curr_bucket += 1;
			}

			if self.curr_bucket >= self.hm.buckets.len() {
				return None;
			}
		}

		let (k, v) = self.hm.buckets[self.curr_bucket]
			.elements
			.index(self.curr_element);
		self.curr_element += 1;
		self.i += 1;
		Some((k, v))
	}

	fn count(self) -> usize {
		self.hm.len() - self.i
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		let len = self.hm.len() - self.i;
		(len, Some(len))
	}
}

// TODO implement DoubleEndedIterator

impl<'m, K: Hash + Eq, V> ExactSizeIterator for Iter<'m, K, V> {
	fn len(&self) -> usize {
		self.hm.len()
	}
}

impl<'m, K: Hash + Eq, V> FusedIterator for Iter<'m, K, V> {}

unsafe impl<'m, K: Hash + Eq, V> TrustedLen for Iter<'m, K, V> {}

impl<K: Eq + Hash + fmt::Display, V: fmt::Display> fmt::Display for HashMap<K, V> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "[")?;

		for (i, (key, value)) in self.iter().enumerate() {
			write!(f, "{}: {}", key, value)?;

			if i + 1 < self.len() {
				write!(f, ", ")?;
			}
		}

		write!(f, "]")
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test_case]
	fn hash_map0() {
		let mut hash_map = HashMap::<u32, u32>::new();

		assert_eq!(hash_map.len(), 0);

		hash_map.insert(0, 0).unwrap();

		assert_eq!(hash_map.len(), 1);
		assert_eq!(*hash_map.get(&0).unwrap(), 0);
		assert_eq!(hash_map[0], 0);

		assert_eq!(hash_map.remove(&0).unwrap(), 0);

		assert_eq!(hash_map.len(), 0);
	}

	#[test_case]
	fn hash_map1() {
		let mut hash_map = HashMap::<u32, u32>::new();

		for i in 0..100 {
			assert_eq!(hash_map.len(), i);

			hash_map.insert(i as _, 0).unwrap();

			assert_eq!(hash_map.len(), i + 1);
			assert_eq!(*hash_map.get(&(i as _)).unwrap(), 0);
			assert_eq!(hash_map[i as _], 0);
		}

		for i in (0..100).rev() {
			assert_eq!(hash_map.len(), i + 1);
			assert_eq!(hash_map.remove(&(i as _)).unwrap(), 0);
			assert_eq!(hash_map.len(), i);
		}
	}
}
