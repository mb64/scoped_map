//! Actual map implementation

use crate::arena::ArenaWrapper;
use crate::*;
use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash, Hasher};
use std::mem;
use typed_arena::{Arena, SubArenaBuilder};

impl<K, V> Default for ScopedMapBase<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> ScopedMapBase<K, V> {
    pub fn new() -> Self {
        Self::with_hasher(Default::default())
    }
}

impl<K, V, S: BuildHasher> ScopedMapBase<K, V, S> {
    pub fn with_hasher(hasher: S) -> Self {
        Self {
            block_arena: Arena::new(),
            entry_arena: Arena::new(),
            hasher,
        }
    }

    pub fn make_map(&self) -> ScopedMap<'_, K, V, S> {
        let generation = 0;
        let block_arena = ArenaWrapper::new(SubArenaBuilder::new(&self.block_arena).build());
        let entry_arena = ArenaWrapper::new(SubArenaBuilder::new(&self.entry_arena).build());
        ScopedMap {
            generation,
            block_arena,
            entry_arena,
            root: ItemRep::empty(),
            hasher: &self.hasher,
        }
    }
}

impl<'a, K, V> ScopedMap<'a, K, V> {
    pub fn new_scope(&self) -> ScopedMap<'_, K, V> {
        let generation = self.generation + 1;
        let block_arena =
            ArenaWrapper::new(SubArenaBuilder::new(&*self.block_arena.inner()).build());
        let entry_arena =
            ArenaWrapper::new(SubArenaBuilder::new(&*self.entry_arena.inner()).build());
        ScopedMap {
            generation,
            block_arena,
            entry_arena,
            root: self.root.clone(),
            hasher: self.hasher,
        }
    }
}

impl<'a, K, V, S: 'a> ScopedMap<'a, K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    pub fn lookup<'map, 'key, Q>(&'map self, key: &'key Q) -> Option<&'map V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        let hash = Self::hash(&self.hasher, key);
        let entry = if let Some(block) = self.root.block() {
            ItemRef::into_ref(block).get_entry_imm(hash)?
        } else if let Some(entry) = self.root.entry() {
            ItemRef::into_ref(entry)
        } else {
            debug_assert!(self.root.is_empty());
            return None;
        };
        entry.lookup(key)
    }

    pub fn insert<'temp>(&'temp mut self, key: K, value: V) {
        let hash = Self::hash(&self.hasher, &key);
        let (mut item, depth): (&'temp mut ItemRep<'a, _, _>, _) =
            Self::get_item_mut(&mut self.root, self.generation, &self.block_arena, hash);
        let old_item = mem::take(item);
        if let Some(old_entry) = old_item.entry() {
            let old_hash = old_entry.hash(self.hasher);
            if old_hash == hash {
                // SAFETY: we use the right generation
                unsafe { old_entry.set(key, value, &self.entry_arena, self.generation, item) };
            } else {
                let new_entry: &'a mut Entry<'a, K, V> = self.entry_arena.alloc(Entry {
                    generation: self.generation,
                    key,
                    value,
                    next: None,
                });
                // Need to make a block, and put both entries in it
                let mut depth = depth;
                let mut new_hash_rest = hash >> depth;
                let mut old_hash_rest = Self::hash(&self.hasher, &old_entry.key) >> depth;
                while depth < 64 {
                    let mut new_block: &'a mut Block<'a, _, _> =
                        self.block_arena.alloc(Block::empty(self.generation));
                    let new_index = new_hash_rest as usize & (BLOCK_SIZE - 1);
                    let old_index = old_hash_rest as usize & (BLOCK_SIZE - 1);
                    if new_index == old_index {
                        new_hash_rest >>= BLOCK_BITS;
                        old_hash_rest >>= BLOCK_BITS;
                        depth += BLOCK_BITS;
                        *item = ItemRep::from_block(new_block);
                        // SAFETY: we use the right generation, and explicitly bound the lifetime
                        // shouldn't even panic, we own this block -- we just made it
                        let new_item: &'temp mut _ = unsafe {
                            &mut ItemRef::promote(item.block().unwrap(), self.generation)
                                .unwrap_or_else(|_| unreachable!())
                                .entries[new_index]
                        };
                        item = new_item;
                        continue;
                    } else {
                        new_block.entries[old_index] =
                            ItemRep::from_entry(ItemRef::into_ref(old_entry));
                        new_block.entries[new_index] = ItemRep::from_entry(new_entry);
                        *item = ItemRep::from_block(new_block);
                        return;
                    }
                }
                unreachable!("Hashes are both unequal and equal: {} {}", hash, old_hash);
            }
        } else {
            debug_assert!(item.is_empty());
            let new_entry: &'a mut Entry<'a, K, V> = self.entry_arena.alloc(Entry {
                generation: self.generation,
                key,
                value,
                next: None,
            });
            *item = ItemRep::from_entry(new_entry);
        }
    }

    #[inline]
    fn hash<Q>(build_hasher: &S, key: &Q) -> u64
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        let mut hasher = build_hasher.build_hasher();
        key.hash(&mut hasher);
        hasher.finish()
    }

    /// Returns the item slot that could be used to store an entry for that hash, and the amount to
    /// shift a hash by to get to that slot
    ///
    /// The result is either:
    ///  * an empty slot
    ///  * a slot with an entry owned by this generation, sharing a prefix of the hash
    ///  * a slot with an entry owned by the previous generation, sharing a prefix of the hash
    fn get_item_mut<'temp>(
        root: &'temp mut ItemRep<'a, K, V>,
        generation: u32,
        block_arena: &'temp ArenaWrapper<'a, Block<'a, K, V>>,
        hash: u64,
    ) -> (&'temp mut ItemRep<'a, K, V>, usize) {
        let mut rest_hash = hash;
        let mut shift_amt = 0;
        let mut item = root;
        loop {
            let block: ItemRef<'temp, _> = match item.block() {
                Some(block) => block,
                None => return (item, shift_amt),
            };
            // SAFETY: we use the right generation, and the lifetime is bounded to 'temp just above
            match unsafe { ItemRef::promote(block, generation) } {
                Ok(mutable_blk) => {
                    // We own this block -- use it
                    let index = rest_hash as usize & (BLOCK_SIZE - 1);
                    item = &mut mutable_blk.entries[index];
                    rest_hash >>= BLOCK_BITS;
                    shift_amt += BLOCK_BITS;
                    // continue
                }
                Err(_) => {
                    // Don't own this block -- gotta copy
                    let (slot, new_shift_amt) =
                        Self::copying_insert(block_arena, generation, item, rest_hash);
                    return (slot, shift_amt + new_shift_amt);
                }
            }
        }
    }

    /// Insert a new empty item for that hash, and return a mutable reference to it, along with the
    /// amount the hash was shifted to get there
    fn copying_insert<'temp>(
        block_arena: &'temp ArenaWrapper<'a, Block<'a, K, V>>,
        generation: u32,
        mut item: &'temp mut ItemRep<'a, K, V>,
        mut rest_hash: u64,
    ) -> (&'temp mut ItemRep<'a, K, V>, usize) {
        let mut shift_amt = 0;
        loop {
            if let Some(block) = item.block() {
                // copy block
                let new_block = block_arena.alloc(Block {
                    generation,
                    entries: block.entries.clone(), // memcpy
                });
                let index = rest_hash as usize & (BLOCK_SIZE - 1);
                *item = ItemRep::from_block(new_block);
                // recurse on the insides of the block
                // SAFETY: we use the right generation and explicitly bound the lifetime
                // should never even panic, we just made this
                let new_item: &'temp mut _ = unsafe {
                    &mut ItemRef::promote(item.block().unwrap(), generation)
                        .unwrap_or_else(|_| unreachable!())
                        .entries[index]
                };
                item = new_item;
                rest_hash >>= BLOCK_BITS;
                shift_amt += BLOCK_BITS;
            } else {
                // not a block -- done!
                // return the item
                return (item, shift_amt);
            }
        }
    }
}

impl<'a, K, V> Block<'a, K, V> {
    fn get_entry_imm<'temp: 'a>(&'temp self, hash: u64) -> Option<&'temp Entry<'a, K, V>> {
        let mut rest_hash = hash;
        let mut block = self;
        loop {
            let index = rest_hash as usize & BLOCK_SIZE - 1;
            let item: &'temp ItemRep<'a, K, V> = &block.entries[index];
            if let Some(new_block) = item.block() {
                block = ItemRef::into_ref(new_block);
                rest_hash >>= BLOCK_BITS;
            } else if let Some(entry) = item.entry() {
                return Some(ItemRef::into_ref(entry));
            } else {
                debug_assert!(item.is_empty());
                return None;
            }
        }
    }
}

impl<'a, K, V> Entry<'a, K, V> {
    /// Gets the hash of this entry
    fn hash<S>(&self, hasher: &S) -> u64
    where
        K: Hash,
        S: BuildHasher,
    {
        let mut h = hasher.build_hasher();
        self.key.hash(&mut h);
        h.finish()
    }

    fn lookup<'temp, Q>(&'temp self, key: &Q) -> Option<&'temp V>
    where
        K: Borrow<Q>,
        Q: Eq,
    {
        let mut entry = self;
        loop {
            if key == entry.key.borrow() {
                return Some(&entry.value);
            }
            match entry.next {
                Some(ref next) => entry = next,
                None => return None,
            }
        }
    }

    /// Possibly mutates self if it's a unique ref, and puts the updated entry in `into`
    ///
    /// Safety: gotta pass the right generation
    unsafe fn set(
        self: ItemRef<'a, Self>,
        key: K,
        value: V,
        arena: &ArenaWrapper<'a, Self>,
        generation: u32,
        into: &mut ItemRep<'a, K, V>,
    ) where
        K: Eq,
    {
        let mut result = None;
        self.set_internal(key, value, arena, generation, &mut result);
        *into = ItemRep::from_entry(ItemRef::into_ref(result.unwrap()));
    }

    /// Safety: gotta pass the right generation
    #[allow(unused_unsafe)]
    unsafe fn set_internal(
        self: ItemRef<'a, Self>,
        key: K,
        value: V,
        arena: &ArenaWrapper<'a, Self>,
        generation: u32,
        mut into: &mut Option<ItemRef<'a, Self>>,
    ) where
        K: Eq,
    {
        // Currently loops thru all owned entries, in case one's the same
        // Might be faster to unconditionally add a link?
        // TODO benchmark

        // entry is always Some(...) when it's used
        // might be a better way to use it?
        let mut entry: Option<ItemRef<'a, Entry<'a, K, V>>> = Some(self);

        let entry = loop {
            match ItemRef::promote(entry.take().unwrap(), generation) {
                Ok(mutable) => {
                    if mutable.key == key {
                        // Mutable, identical -- update in place
                        mutable.value = value;
                        mutable.key = key;
                        *into = Some(ItemRef::from_mut(mutable));
                        return;
                    } else if let Some(next) = mutable.next.take() {
                        // Mutable, has continuation -- loop on the continuation
                        *into = Some(ItemRef::from_mut(mutable));
                        // SAFETY: the reference is unique, we just put a mutable reference there
                        into = unsafe {
                            &mut ItemRef::promote_mut(into.as_mut().unwrap(), generation)
                                .unwrap()
                                .next
                        };
                        entry = Some(next);
                        continue;
                    } else {
                        // end of chain -- add new link
                        break ItemRef::from_mut(mutable);
                    }
                }
                Err(entry_again) => {
                    // Immutable -- add new link
                    break entry_again;
                }
            }
        };
        // add new link
        let new_entry = arena.alloc(Entry {
            generation,
            key,
            value,
            next: Some(entry),
        });
        *into = Some(ItemRef::from_mut(new_entry));
        return;
    }
}
