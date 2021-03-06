//! A container designed to hold info about variables
//!
//! Represented as an arena-allocated persistant hash table
//! Lookup is technically O(log(n)), but the base of the logarithm is large
//! enough for it to be practicaly constant-time.

#![feature(arbitrary_self_types)]
#![cfg_attr(feature = "benching", feature(custom_test_frameworks))]
#![cfg_attr(feature = "benching", test_runner(criterion::runner))]

mod arena;
mod map;
mod structs;

pub(crate) const BLOCK_BITS: usize = 4;
pub(crate) const BLOCK_SIZE: usize = 1 << BLOCK_BITS;

pub(crate) use structs::{Block, Entry, ItemRef, ItemRep};
pub use structs::{ScopedMap, ScopedMapBase};

#[cfg(test)]
mod tests;
