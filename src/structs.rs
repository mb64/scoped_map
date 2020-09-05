//! Datastructures

use crate::arena::ArenaWrapper;
use crate::BLOCK_SIZE;

use ahash::RandomState;
use std::marker::PhantomData;
use std::ops::Deref;
use std::ptr::{self, NonNull};
use typed_arena::Arena;

/// A single hashmap item.
///
/// It's stored in `ItemRep<'a>` using a tagged pointer, but accessible with the `.item()` and
/// `.set()` methods
pub struct ItemRep<'a, K: 'static, V: 'static> {
    ptr: *mut (),
    _marker: PhantomData<(&'a Block<'a, K, V>, &'a Entry<'a, K, V>)>,
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

    pub fn entry(&self) -> Option<ItemRef<'a, Entry<'a, K, V>>> {
        // SAFETY: reference always valid as shared refs
        if (self.ptr as usize & 1) == 0 {
            Some(ItemRef::new(NonNull::new(self.ptr)?.cast()))
        } else {
            None
        }
    }

    pub fn block(&self) -> Option<ItemRef<'a, Block<'a, K, V>>> {
        // SAFETY: reference always valid as shared refs
        if (self.ptr as usize & 1) == 1 {
            let ptr = (self.ptr as usize & !1) as *mut _;
            Some(ItemRef::new(NonNull::new(ptr)?))
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

    // FIXME: Safety: should this be unsafe?
    pub fn from_block(block: &'a Block<'a, K, V>) -> Self {
        Self {
            ptr: (block as *const _ as usize | 1) as *mut _,
            _marker: PhantomData,
        }
    }

    // FIXME: Safety: should this be unsafe?
    pub fn from_entry(entry: &'a Entry<'a, K, V>) -> Self {
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

/// A reference to an item, promotable to a mutable reference if it's unique
pub struct ItemRef<'a, T> {
    // Invariant: ptr is always a valid reference
    ptr: NonNull<T>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T> ItemRef<'a, T> {
    fn new(ptr: NonNull<T>) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Safety: provide the right generation
    pub unsafe fn promote(this: Self, generation: u32) -> Result<&'a mut T, Self>
    where
        T: Item,
    {
        debug_assert!(generation >= this.generation());
        if this.generation() == generation {
            Ok(&mut *this.ptr.as_ptr())
        } else {
            Err(this)
        }
    }

    /// Safety: provide the right generation
    pub unsafe fn promote_mut(this: &mut Self, generation: u32) -> Option<&mut T>
    where
        T: Item,
    {
        debug_assert!(generation >= this.generation());
        if this.generation() == generation {
            Some(&mut *this.ptr.as_ptr())
        } else {
            None
        }
    }

    pub fn into_ref(self) -> &'a T {
        // SAFETY: pointer is always a valid reference
        // No way to get from this reference to a mutable pointer
        unsafe { &*self.ptr.as_ptr() }
    }

    pub fn from_mut(val: &'a mut T) -> Self {
        // SAFETY: reference will never be null
        Self {
            ptr: unsafe { NonNull::new_unchecked(val as *mut _) },
            _marker: PhantomData,
        }
    }
}

impl<'temp, T> Deref for ItemRef<'temp, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: ptr is always a valid reference
        unsafe { self.ptr.as_ref() }
    }
}

pub trait Item {
    fn generation(&self) -> u32;
}

pub struct Block<'a, K: 'static, V: 'static> {
    pub generation: u32,
    pub entries: [ItemRep<'a, K, V>; BLOCK_SIZE],
}

impl<'a, K, V> Item for Block<'a, K, V> {
    fn generation(&self) -> u32 {
        self.generation
    }
}

impl<'a, K: 'static, V: 'static> Block<'a, K, V> {
    ///// Promotes an immutable reference to a mutable one, if it's unique
    ///// A reference with the same generation is guaranteed unique
    /////
    ///// Safety: gotta pass the right generation, and explicitly bound result lifetime
    //pub unsafe fn promote<'temp>(&'temp self, generation: u32) -> Option<&'temp mut Self> {
    //    debug_assert!(generation >= self.generation);
    //    if self.generation == generation {
    //        Some(&mut *(self as *const _ as *mut _))
    //    } else {
    //        None
    //    }
    //}

    pub fn empty(generation: u32) -> Self {
        let entries = <[ItemRep<'a, K, V>; BLOCK_SIZE] as Default>::default();
        Self {
            generation,
            entries,
        }
    }
}

pub struct Entry<'a, K: 'static, V: 'static> {
    pub generation: u32,
    pub key: K,
    pub value: V,
    /// Invariant: they all have the same hash
    pub next: Option<ItemRef<'a, Entry<'a, K, V>>>,
}

impl<'a, K, V> Item for Entry<'a, K, V> {
    fn generation(&self) -> u32 {
        self.generation
    }
}

impl<'a, K, V> Entry<'a, K, V> {
    ///// Promotes an immutable reference to a mutable one, if it's unique
    ///// A reference with the same generation is guaranteed unique
    /////
    ///// Safety: gotta pass the right generation, and explicitly bound result lifetime
    //pub unsafe fn promote<'temp>(&'temp self, generation: u32) -> Option<&'temp mut Self> {
    //    if self.generation == generation {
    //        Some(&mut *(self as *const _ as *mut _))
    //    } else {
    //        None
    //    }
    //}
}

pub struct ScopedMapBase<K: 'static, V: 'static, S: 'static = RandomState> {
    pub(crate) block_arena: Arena<Block<'static, K, V>>,
    pub(crate) entry_arena: Arena<Entry<'static, K, V>>,
    pub(crate) hasher: S,
}

pub struct ScopedMap<'a, K: 'static, V: 'static, S: 'static = RandomState> {
    pub(crate) generation: u32,
    pub(crate) block_arena: ArenaWrapper<'a, Block<'a, K, V>>,
    pub(crate) entry_arena: ArenaWrapper<'a, Entry<'a, K, V>>,
    pub(crate) root: ItemRep<'a, K, V>,
    pub(crate) hasher: &'a S,
}
