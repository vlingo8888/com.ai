//! The few `bstr` helpers the pre-processors need (`ByteSlice::replace`,
//! `ByteSlice::contains_str`, `ByteVec::replace_range`), with bstr's semantics.

use memchr::memmem;
use std::ops::RangeBounds;

pub trait ByteSlice {
    /// Replace every non-overlapping occurrence of `from` (left to right).
    fn replace<F: AsRef<[u8]>, T: AsRef<[u8]>>(&self, from: F, to: T) -> Vec<u8>;
    fn contains_str<N: AsRef<[u8]>>(&self, needle: N) -> bool;
}

impl ByteSlice for [u8] {
    fn replace<F: AsRef<[u8]>, T: AsRef<[u8]>>(&self, from: F, to: T) -> Vec<u8> {
        let (from, to) = (from.as_ref(), to.as_ref());
        debug_assert!(!from.is_empty());
        let mut out = Vec::with_capacity(self.len());
        let mut last = 0;
        for start in memmem::find_iter(self, from) {
            if start < last {
                continue;
            }
            out.extend_from_slice(&self[last..start]);
            out.extend_from_slice(to);
            last = start + from.len();
        }
        out.extend_from_slice(&self[last..]);
        out
    }

    fn contains_str<N: AsRef<[u8]>>(&self, needle: N) -> bool {
        memmem::find(self, needle.as_ref()).is_some()
    }
}

pub trait ByteVec {
    fn replace_range<R: RangeBounds<usize>, B: AsRef<[u8]>>(&mut self, range: R, replace_with: B);
}

impl ByteVec for Vec<u8> {
    fn replace_range<R: RangeBounds<usize>, B: AsRef<[u8]>>(&mut self, range: R, replace_with: B) {
        self.splice(range, replace_with.as_ref().iter().copied());
    }
}
