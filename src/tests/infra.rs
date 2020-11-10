//! Infrastructure for running tests
//!
//! Doesn't have any actual tests, just the tools to run them

use crate::*;
use rand::distributions::{Distribution, Standard};
use rand::prelude::*;
use std::borrow::Borrow;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::rc::Rc;

pub(crate) trait Map<'a, K, V> {
    fn lookup<'map, 'key, Q>(&'map self, key: &'key Q) -> Option<&'map V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash;
    fn insert(&mut self, key: K, value: V);
    fn new_scope(&'a self) -> Self;
}

impl<'a, K: 'static, V: 'static> Map<'a, K, V> for ScopedMap<'a, K, V>
where
    K: Eq + Hash,
{
    fn lookup<'map, 'key, Q>(&'map self, key: &'key Q) -> Option<&'map V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.lookup(key)
    }

    fn insert(&mut self, key: K, value: V) {
        self.insert(key, value);
    }

    fn new_scope(&'a self) -> Self {
        self.new_scope()
    }
}

impl<'a, K, V, S> Map<'a, K, V> for im_rc::HashMap<K, V, S>
where
    K: Clone + Eq + Hash,
    V: Clone,
    S: std::hash::BuildHasher,
{
    fn lookup<'map, 'key, Q>(&'map self, key: &'key Q) -> Option<&'map V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.get(key)
    }

    fn insert(&mut self, key: K, value: V) {
        self.insert(key, value);
    }

    fn new_scope(&'a self) -> Self {
        self.clone()
    }
}

/// A functional specification for the map
pub struct Spec<K, V> {
    head: Option<Rc<Node<K, V>>>,
}

struct Node<K, V> {
    key: K,
    value: V,
    next: Option<Rc<Node<K, V>>>,
}

impl<K, V> Spec<K, V> {
    pub fn new() -> Self {
        Self { head: None }
    }
}

impl<'a, K: Eq, V> Map<'a, K, V> for Spec<K, V> {
    fn insert(&mut self, key: K, value: V) {
        let new_node = Node {
            key,
            value,
            next: self.head.take(),
        };
        self.head = Some(Rc::new(new_node));
    }

    fn lookup<'map, 'key, Q>(&'map self, key: &'key Q) -> Option<&'map V>
    where
        K: Borrow<Q>,
        Q: Eq,
    {
        let mut node = &self.head;
        while let Some(n) = node {
            if n.key.borrow() == key {
                return Some(&n.value);
            }
            node = &n.next;
        }
        return None;
    }

    fn new_scope(&'a self) -> Self {
        Self {
            head: self.head.clone(),
        }
    }
}

// Use a tuple to check results between two maps
impl<'a, K, V, A, B> Map<'a, K, V> for (A, B)
where
    K: Clone,
    V: Clone + Eq + Debug,
    A: Map<'a, K, V>,
    B: Map<'a, K, V>,
{
    fn insert(&mut self, key: K, value: V) {
        self.0.insert(key.clone(), value.clone());
        self.1.insert(key, value);
    }
    fn lookup<'map, 'key, Q>(&'map self, key: &'key Q) -> Option<&'map V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
    {
        let (a, b) = (self.0.lookup(key), self.1.lookup(key));
        assert_eq!(a, b);
        a
    }

    fn new_scope(&'a self) -> Self {
        (self.0.new_scope(), self.1.new_scope())
    }
}

pub enum MapCmd<K> {
    NewScope,
    OldScope,
    Lookup(K, u32), // u32 depth: 0 is current
    Insert(K, u32), // u32 value
}

impl<K> MapCmd<K> {
    pub fn map<B>(self, f: impl FnOnce(K) -> B) -> MapCmd<B> {
        use MapCmd::*;
        match self {
            NewScope => NewScope,
            OldScope => OldScope,
            Lookup(k, v) => Lookup(f(k), v),
            Insert(k, v) => Insert(f(k), v),
        }
    }
}

pub struct RandCmds<T, D> {
    rng: SmallRng,
    depth: u32,
    dist: D,
    _marker: PhantomData<T>,
}

impl<T, D> RandCmds<T, D> {
    pub fn new(dist: D) -> Self {
        Self {
            rng: SmallRng::from_entropy(),
            depth: 1,
            dist,
            _marker: PhantomData,
        }
    }
}

impl<T, D> Iterator for RandCmds<T, D>
where
    D: Distribution<T>,
{
    type Item = MapCmd<T>;
    fn next(&mut self) -> Option<Self::Item> {
        let x = self.rng.gen::<f32>();
        const SCOPE_FRAC: f32 = 0.005;
        const INSERT_FRAC: f32 = 0.05;

        if x < SCOPE_FRAC {
            // Change scope
            let cutoff = self.depth as f32 * SCOPE_FRAC / 32.;
            if x > cutoff {
                self.depth += 1;
                Some(MapCmd::NewScope)
            } else {
                self.depth -= 1;
                Some(MapCmd::OldScope)
            }
        } else if x > 1. - INSERT_FRAC {
            // Insert
            let key = self.dist.sample(&mut self.rng);
            let value = self.rng.gen::<u32>();
            Some(MapCmd::Insert(key, value))
        } else {
            // Lookup
            let key = self.dist.sample(&mut self.rng);
            let depth = self.rng.gen_range(0, self.depth);
            Some(MapCmd::Lookup(key, depth))
        }
    }
}

// Unfortunately this doesn't work -- there's no way to specify lifetime variance with trait magic
// fn do_cmds<'a, M>(mut map_list: List<'a, M>, cmds: &mut impl Iterator<Item = MapCmd<u8>>)
// where
//     M: Map<'a, u8, u32>,

macro_rules! random_base {
    (
        item: $itemty:ty;
        map: $maptype:ty;
    ) => {
        use super::*;
        #[allow(unused_imports)]
        use criterion::{black_box, Criterion};
        #[allow(unused_imports)]
        use criterion_macro::criterion;

        struct List<'a, M> {
            current: M,
            prev: Option<&'a List<'a, M>>,
        }

        type M<'a> = $maptype;
        fn do_cmds<'a>(
            mut map_list: List<'a, M<'a>>,
            cmds: &mut impl Iterator<Item = MapCmd<$itemty>>,
        ) {
            loop {
                let cmd = match cmds.next() {
                    Some(c) => c,
                    None => return,
                };
                match cmd {
                    MapCmd::NewScope => {
                        let new_map = map_list.current.new_scope();
                        do_cmds(
                            List {
                                current: new_map,
                                prev: Some(&map_list),
                            },
                            cmds,
                        );
                    }
                    MapCmd::OldScope => return,
                    MapCmd::Lookup(key, depth) => {
                        let mut l = &map_list;
                        for _ in 0..depth {
                            l = l.prev.unwrap();
                        }
                        let _ = l.current.lookup(&key);
                    }
                    MapCmd::Insert(key, value) => {
                        map_list.current.insert(key, value);
                    }
                }
            }
        }
    };
}

macro_rules! random_test {
    (
        name: $name:ident;
        item: $itemty:ty;
        map: $maptype:ty;
        init: $init:expr;
        normally: $normal:expr;
        miri: $miri:expr;
    ) => {
        #[cfg(not(feature = "benching"))]
        mod $name {
            random_base! {
                item: $itemty;
                map: $maptype;
            }

            #[test]
            fn random() {
                #[cfg(not(miri))]
                const ITERS: usize = $normal;
                #[cfg(miri)]
                const ITERS: usize = $miri;

                let mut iter = RandCmds::new(Standard).take(ITERS);
                let map_base = ScopedMapBase::<$itemty, u32>::new();
                let map = map_base.make_map();
                do_cmds(
                    List {
                        current: ($init)(map),
                        prev: None,
                    },
                    &mut iter,
                );
            }
        }
    };
}

macro_rules! random_bench {
    (
        name: $name:ident $strname:expr;
        item: $itemty:ty;
        map: $maptype:ty;
        init: $init:expr;
        iters: $iters:expr;
    ) => {
        #[cfg(feature = "benching")]
        mod $name {
            random_base! {
                item: $itemty;
                map: $maptype;
            }

            #[criterion]
            fn random(c: &mut Criterion) {
                const ITERS: usize = $iters;

                c.bench_function($strname, |b| {
                    let map_base = ScopedMapBase::<$itemty, u32>::new();

                    b.iter(|| {
                        let mut iter = RandCmds::new(Standard).take(ITERS);
                        let map = map_base.make_map();
                        do_cmds(
                            List {
                                current: ($init)(map),
                                prev: None,
                            },
                            &mut iter,
                        );
                    });
                });
            }
        }
    };
}

macro_rules! random_bench_zipf {
    (
        name: $name:ident $strname:expr;
        map: $maptype:ty;
        init: $init:expr;
        iters: $iters:expr;
        zipf count: $zipf_count:expr;
    ) => {
        #[cfg(feature = "benching")]
        mod $name {
            random_base! {
                item: &'static str;
                map: $maptype;
            }

            #[criterion]
            fn random(c: &mut Criterion) {
                const ITERS: usize = $iters;
                const ZIPF_COUNT: usize = $zipf_count;

                let items = (0..=ZIPF_COUNT)
                    .map(|x| Box::leak(format!("{}", x * x).into_boxed_str()) as &'static str)
                    .collect::<Vec<&'static str>>();

                c.bench_function($strname, |b| {
                    let map_base = ScopedMapBase::<&'static str, u32>::new();

                    b.iter(|| {
                        let zipf = zipf::ZipfDistribution::new(ZIPF_COUNT, 1.).unwrap();
                        let mut iter = RandCmds::new(zipf)
                            .map(|cmd| cmd.map(|x| items[x]))
                            .take(ITERS);
                        let map = map_base.make_map();
                        do_cmds(
                            List {
                                current: ($init)(map),
                                prev: None,
                            },
                            &mut iter,
                        );
                    });
                });
            }
        }
    };
}

/// A struct for testing hash collisions
///
/// It's got a `u16`, but only hashes 2 of the bits
///
/// The `Standard` distribution gives numbers 0..1024
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BadHash(pub u16);

impl Distribution<BadHash> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> BadHash {
        // Only take 10 of the bits
        BadHash(rng.gen::<u16>() & 0x03ff)
    }
}

impl Hash for BadHash {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        // Only 2 bits
        (self.0 & 3).hash(hasher);
    }
}
