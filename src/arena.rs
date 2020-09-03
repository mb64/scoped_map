//! A wrapper for the arena, with less restrictive lifetimes
//!
//! TODO: some better formalism for why this is safe
//! For now, it seems to work, and Miri's happy, but I don't like it very much

use std::marker::PhantomData;
use std::mem;
use typed_arena::SubArena;

pub struct ArenaWrapper<'a, T> {
    // INVARIANT: SubArena has the same size/align for all types
    inner: mem::ManuallyDrop<SubArena<'static, ()>>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T> Drop for ArenaWrapper<'a, T> {
    fn drop(&mut self) {
        unsafe {
            mem::ManuallyDrop::drop(mem::transmute::<
                &mut mem::ManuallyDrop<SubArena<'static, ()>>,
                &mut mem::ManuallyDrop<SubArena<'a, T>>,
            >(&mut self.inner));
        }
    }
}

impl<'a, T> ArenaWrapper<'a, T> {
    pub fn alloc(&self, item: T) -> &'a mut T {
        // SAFETY: Since there is no other way to access the contents than through this function,
        // it's safe to allocate shorter-lived items
        // Either way, it's not public, so any errors are limited to just this crate
        //
        // oh shit Drop can access the data
        // not sure if there's any way to exploit this tho
        // ... Rustc seems to detect this? idk seems like black magic
        unsafe {
            let arena: &'a SubArena<'a, T> =
                mem::transmute::<&SubArena<'static, ()>, &'a SubArena<'a, T>>(&self.inner);
            arena.alloc(item)
        }
    }

    /// Yeah tbh I'm not sure how Rustc's able to figure out that this is bad, but it does
    ///
    /// ```compile_fail
    /// fn bad() {
    ///     struct Bad<'a>(&'static str, Option<&'a Bad<'a>>);
    ///     impl<'a> Drop for Bad<'a> {
    ///         fn drop(&mut self) {
    ///             self.0 = "bad";
    ///             if let Some(a) = self.1.as_ref() {
    ///                 println!("self.1.0 is {}", a.0);
    ///             }
    ///         }
    ///     }
    ///     let arena = Box::leak(Box::new(Arena::new()));
    ///
    ///     let base = Bad("hi there", None);
    ///     let next = Bad("another", None);
    ///
    ///     let mut sub_arena: ArenaWrapper<'static, _> = ArenaWrapper::new(SubArena::new(arena));
    ///     let next = sub_arena.alloc(next);
    ///     let base = sub_arena.alloc(base);
    ///     next.1 = Some(&*base);
    /// }
    /// ```
    pub fn new(inner: SubArena<'a, T>) -> Self {
        Self {
            inner: unsafe { mem::transmute(inner) },
            _marker: PhantomData,
        }
    }

    pub fn inner(&self) -> &SubArena<'a, T> {
        unsafe { mem::transmute(&self.inner) }
    }
}

#[cfg(test)]
mod sanity {
    use super::*;

    #[test]
    fn sub_arena_size_constant() {
        assert_eq!(
            mem::size_of::<SubArena<'static, ()>>(),
            mem::size_of::<SubArena<'static, u128>>()
        );
        assert_eq!(
            mem::align_of::<SubArena<'static, u8>>(),
            mem::align_of::<SubArena<'static, u128>>()
        );
    }
}
