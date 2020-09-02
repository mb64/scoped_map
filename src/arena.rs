//! A wrapper for the arena, with less restrictive lifetimes

use std::mem;
use typed_arena::{Arena, SubArena};

pub struct ArenaWrapper<'a, T> {
    inner: SubArena<'a, T>,
}

impl<'a, T> ArenaWrapper<'a, T> {
    pub fn alloc(&self, item: T) -> &'a mut T {
        // SAFETY: TODO, explain why this is safe
        unsafe {
            let arena: &'a Arena<T> = mem::transmute(&*self.inner);
            arena.alloc(item)
        }
    }

    pub fn new(inner: SubArena<'a, T>) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &SubArena<'a, T> {
        &self.inner
    }
}
