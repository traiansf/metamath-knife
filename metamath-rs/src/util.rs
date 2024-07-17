//! Support functions that don't belong anywhere else or use unsafe code.

use fnv::FnvHasher;
use std::collections;
use std::hash::BuildHasherDefault;
use std::ptr;

/// Type alias for hashmaps to allow swapping out the implementation.
pub(crate) type HashMap<K, V> = collections::HashMap<K, V, BuildHasherDefault<FnvHasher>>;
/// Type alias for hashsets to allow swapping out the implementation.
pub(crate) type HashSet<K> = collections::HashSet<K, BuildHasherDefault<FnvHasher>>;

/// Empty a vector of a POD type without checking each element for droppability.
pub(crate) fn fast_clear<T: Copy>(vec: &mut Vec<T>) {
    unsafe {
        vec.set_len(0);
    }
}

// emprically, *most* copies in the verifier where fast_extend and extend_from_within
// are used are 1-2 bytes
unsafe fn short_copy<T>(src: *const T, dst: *mut T, count: usize) {
    match count {
        1 => ptr::write(dst, ptr::read(src)),
        2 => ptr::write(dst.cast::<[T; 2]>(), ptr::read(src.cast())),
        _ => ptr::copy_nonoverlapping(src, dst, count),
    }
}

/// Appends a POD slice to a vector with a simple `memcpy`.
pub(crate) fn fast_extend<T: Copy>(vec: &mut Vec<T>, other: &[T]) {
    vec.reserve(other.len());
    unsafe {
        let len = vec.len();
        short_copy(other.as_ptr(), vec.as_mut_ptr().add(len), other.len());
        vec.set_len(len + other.len());
    }
}
