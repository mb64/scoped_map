//! Actual map implementation

use crate::arena::ArenaWrapper;
use crate::*;
use either::{Either, Left, Right};
use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash, Hasher};
use typed_arena::{Arena, SubArenaBuilder};

impl<K: 'static, V: 'static> Default for ScopedMapBase<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: 'static, V: 'static> ScopedMapBase<K, V> {
    pub fn new() -> Self {
        Self::with_hasher(Default::default())
    }
}

impl<K: 'static, V: 'static, S: BuildHasher> ScopedMapBase<K, V, S> {
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
    pub fn lookup<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        let hash = Self::hash(&self.hasher, key);
        let entry = if let Some(block) = self.root.block() {
            // SAFETY: root is always a valid reference, and sometimes a valid mutable reference
            unsafe { &*block.as_ptr() }.get_entry_imm(hash)?
        } else if let Some(entry) = self.root.entry() {
            // SAFETY: same as above
            unsafe { &*entry.as_ptr() }
        } else {
            debug_assert!(self.root.is_empty());
            return None;
        };
        if key == entry.key.borrow() {
            Some(&entry.value)
        } else {
            None
        }
    }

    pub fn insert<'temp>(&'temp mut self, key: K, value: V) {
        let hash = Self::hash(&self.hasher, &key);
        let (item, depth) =
            Self::get_item_mut(&mut self.root, self.generation, &self.block_arena, hash);
        if let Some(mut entry) = item.entry() {
            // SAFETY: OK to immutably borrow
            if unsafe { entry.as_ref().generation == self.generation && entry.as_ref().key == key }
            {
                // SAFETY: ok to mutably borrow since it's in the same generation
                unsafe {
                    entry.as_mut().key = key;
                    entry.as_mut().value = value;
                }
            } else {
                // SAFETY: OK to immutably borrow
                let old_entry = unsafe { &*entry.as_ptr() };
                let new_entry: &'a mut Entry<K, V> = self.entry_arena.alloc(Entry {
                    generation: self.generation,
                    key,
                    value,
                });
                // Need to make a block, and put both entries in it
                let mut item = item;
                let mut depth = depth;
                let mut new_hash_rest = hash >> depth;
                let mut old_hash_rest = Self::hash(&self.hasher, &old_entry.key) >> depth;
                while depth < 64 {
                    if new_hash_rest == old_hash_rest {
                        todo!("TODO: handle hash collisions");
                    }
                    let mut new_block = self.block_arena.alloc(Block::empty(self.generation));
                    let new_index = new_hash_rest as usize & (BLOCK_SIZE - 1);
                    let old_index = old_hash_rest as usize & (BLOCK_SIZE - 1);
                    if new_index == old_index {
                        new_hash_rest >>= BLOCK_BITS;
                        old_hash_rest >>= BLOCK_BITS;
                        depth += BLOCK_BITS;
                        *item = ItemRep::from_block(new_block);
                        // SAFETY: shouldn't even panic, we own this block -- we just made it
                        item = unsafe {
                            &mut Block::from_ptr(item.block().unwrap(), self.generation)
                                .right_or_else(|_| unreachable!())
                                .entries[new_index]
                        };
                    // continue
                    } else {
                        new_block.entries[old_index] = ItemRep::from_entry(old_entry);
                        new_block.entries[new_index] = ItemRep::from_entry(new_entry);
                        *item = ItemRep::from_block(new_block);
                        return;
                    }
                }
                unreachable!("I think the collision check inside the loop should do it?");
            }
        } else {
            debug_assert!(item.is_empty());
            let new_entry: &'a mut Entry<K, V> = self.entry_arena.alloc(Entry {
                generation: self.generation,
                key,
                value,
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

    // #[inline]
    // fn root(&self) -> &Block<'a, K, V> {
    //     // SAFETY: root is always a valid reference, and sometimes a valid mutable reference
    //     unsafe { self.root.as_ref() }
    // }

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
            let block = match item.block() {
                Some(block) => block,
                None => return (item, shift_amt),
            };
            // SAFETY: constrained to temporary lifetime
            let is_mutable: Either<&'temp _, &'temp mut _> =
                unsafe { Block::from_ptr(block, generation) };
            match is_mutable {
                Left(_) => {
                    let (slot, new_shift_amt) =
                        Self::copying_insert(block_arena, generation, item, rest_hash);
                    return (slot, shift_amt + new_shift_amt);
                }
                Right(blk) => {
                    let index = rest_hash as usize & (BLOCK_SIZE - 1);
                    item = &mut blk.entries[index];
                    rest_hash >>= BLOCK_BITS;
                    shift_amt += BLOCK_BITS;
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
                // SAFETY: references always safe, bounded by type annotation
                let block_ref: &'temp Block<'a, K, V> = unsafe { &*block.as_ptr() };
                // copy block
                let new_block = block_arena.alloc(Block {
                    generation,
                    entries: block_ref.entries.clone(),
                });
                let index = rest_hash as usize & (BLOCK_SIZE - 1);
                *item = ItemRep::from_block(new_block);
                // recurse on the insides of the block
                // SAFETY: should never panic, we just made this to be the right generation
                item = unsafe {
                    &mut Block::from_ptr(item.block().unwrap(), generation)
                        .right_or_else(|_| unreachable!())
                        .entries[index]
                };
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
    fn get_entry_imm<'temp: 'a>(&'temp self, hash: u64) -> Option<&'temp Entry<K, V>> {
        let mut rest_hash = hash;
        let mut block = self;
        loop {
            let index = rest_hash as usize & BLOCK_SIZE - 1;
            let item: &'temp ItemRep<'a, K, V> = &block.entries[index];
            if let Some(new_block) = item.block() {
                // SAFETY: block only reference valid blocks
                block = unsafe { &*new_block.as_ptr() };
                rest_hash >>= BLOCK_BITS;
            } else if let Some(entry) = item.entry() {
                // SAFETY: blocks only reference valid entries, returned value cannot outlive 'a
                return Some(unsafe { &*entry.as_ptr() });
            } else {
                // TODO: handle hash collisions
                debug_assert!(item.is_empty());
                return None;
            }
        }
    }
}
