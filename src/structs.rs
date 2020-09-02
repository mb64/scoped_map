//! Datastructures

use crate::arena::ArenaWrapper;
use crate::BLOCK_SIZE;

use ahash::RandomState;
use either::{Either, Left, Right};
use std::marker::PhantomData;
use std::ptr::{self, NonNull};
use typed_arena::Arena;

/// A single hashmap item.
///
/// It's stored in `ItemRep<'a>` using a tagged pointer, but accessible with the `.item()` and
/// `.set()` methods
pub struct ItemRep<'a, K, V> {
    ptr: *mut (),
    _marker: PhantomData<(&'a Block<'a, K, V>, &'a Entry<K, V>)>,
}

impl<'a, K, V> Clone for ItemRep<'a, K, V> {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            _marker: self._marker,
        }
    }
}

impl<'a, K, V> ItemRep<'a, K, V> {
    pub fn is_empty(&self) -> bool {
        self.ptr.is_null()
    }

    pub fn entry(&self) -> Option<NonNull<Entry<K, V>>> {
        if (self.ptr as usize & 1) == 0 {
            NonNull::new(self.ptr.cast())
        } else {
            None
        }
    }

    pub fn block(&self) -> Option<NonNull<Block<'a, K, V>>> {
        if (self.ptr as usize & 1) == 1 {
            let ptr = (self.ptr as usize & !1) as *mut _;
            NonNull::new(ptr)
        } else {
            None
        }
    }

    pub fn empty() -> Self {
        Self {
            ptr: ptr::null_mut(),
            _marker: PhantomData,
        }
    }

    pub fn from_block(block: &'a Block<'a, K, V>) -> Self {
        Self {
            ptr: (block as *const _ as usize | 1) as *mut _,
            _marker: PhantomData,
        }
    }

    pub fn from_entry(entry: &'a Entry<K, V>) -> Self {
        Self {
            ptr: entry as *const _ as *mut (),
            _marker: PhantomData,
        }
    }
}

impl<'a, K, V> Default for ItemRep<'a, K, V> {
    fn default() -> Self {
        Self::empty()
    }
}

pub struct Block<'a, K, V> {
    pub generation: u32,
    pub entries: [ItemRep<'a, K, V>; BLOCK_SIZE],
}

impl<'a, K, V> Block<'a, K, V> {
    /// This should typically be safe to call
    /// TODO Maybe make a `BlockRef` and `EntryRef` type, that allows to do it safely?
    /// still requires correct passing of generations, so nah :/
    pub unsafe fn from_ptr(ptr: NonNull<Self>, cur_gen: u32) -> Either<&'a Self, &'a mut Self> {
        let block_gen = ptr.as_ref().generation;
        debug_assert!(
            block_gen <= cur_gen,
            "References are never from older generations to newer generations"
        );
        if block_gen == cur_gen {
            Right(&mut *ptr.as_ptr())
        } else {
            Left(&*ptr.as_ptr())
        }
    }

    pub fn empty(generation: u32) -> Self {
        let entries = <[ItemRep<'a, K, V>; BLOCK_SIZE] as Default>::default();
        Self {
            generation,
            entries,
        }
    }
}

pub struct Entry<K, V> {
    pub generation: u32,
    pub key: K,
    pub value: V,
}

impl<K, V> Entry<K, V> {
    /// Hella unsafe, unspecified return lifetime
    ///
    /// If you call this, make sure to specify the right lifetime
    pub unsafe fn from_ptr<'a>(ptr: NonNull<Self>, cur_gen: u32) -> Either<&'a Self, &'a mut Self> {
        let entry_gen = ptr.as_ref().generation;
        debug_assert!(
            entry_gen <= cur_gen,
            "References are never from older generations to newer generations"
        );
        if entry_gen == cur_gen {
            Right(&mut *ptr.as_ptr())
        } else {
            Left(&*ptr.as_ptr())
        }
    }
}

pub struct ScopedMapBase<K: 'static, V: 'static, S = RandomState> {
    pub(crate) block_arena: Arena<Block<'static, K, V>>,
    pub(crate) entry_arena: Arena<Entry<K, V>>,
    pub(crate) hasher: S,
}

pub struct ScopedMap<'a, K, V, S = RandomState> {
    pub(crate) generation: u32,
    pub(crate) block_arena: ArenaWrapper<'a, Block<'a, K, V>>,
    pub(crate) entry_arena: ArenaWrapper<'a, Entry<K, V>>,
    pub(crate) root: ItemRep<'a, K, V>,
    pub(crate) hasher: &'a S,
}
