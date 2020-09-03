use crate::*;
use rand::prelude::*;
use std::borrow::Borrow;
use std::fmt::Debug;
use std::hash::Hash;
use std::rc::Rc;

// Make sure it's covariant in lifetime
fn _variance_check<'a>(map: &'a ScopedMap<'static, u8, u8>) {
    let _other_map: &'a ScopedMap<'a, u8, u8> = map;
}

trait Map<'a, K, V> {
    fn lookup<'map, 'key, Q>(&'map self, key: &'key Q) -> Option<&'map V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash;
    fn insert(&mut self, key: K, value: V);
    fn new_scope(&'a self) -> Self;
}

impl<'a, K, V> Map<'a, K, V> for ScopedMap<'a, K, V>
where
    K: Eq + Hash + Debug,
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

/// A functional specification for the map
struct Spec<K, V> {
    head: Option<Rc<Node<K, V>>>,
}

struct Node<K, V> {
    key: K,
    value: V,
    next: Option<Rc<Node<K, V>>>,
}

impl<K, V> Spec<K, V> {
    fn new() -> Self {
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

enum MapCmd<K> {
    NewScope,
    OldScope,
    Lookup(K, u32), // u32 depth: 0 is current
    Insert(K, u32), // u32 value
}

struct RandCmds {
    rng: SmallRng,
    depth: u32,
}

impl RandCmds {
    fn new() -> Self {
        Self {
            rng: SmallRng::from_entropy(),
            depth: 1,
        }
    }
}

impl Iterator for RandCmds {
    type Item = MapCmd<u8>;
    fn next(&mut self) -> Option<Self::Item> {
        let x = self.rng.gen::<f32>();
        if x < 0.1 {
            // Change scope
            let cutoff = self.depth as f32 * 0.1 / 32.;
            if x > cutoff {
                self.depth += 1;
                Some(MapCmd::NewScope)
            } else {
                self.depth -= 1;
                Some(MapCmd::OldScope)
            }
        } else if x < 0.9 {
            // Lookup
            let key = self.rng.gen::<u8>();
            let depth = self.rng.gen_range(0, self.depth);
            Some(MapCmd::Lookup(key, depth))
        } else {
            // Insert
            let key = self.rng.gen::<u8>();
            let value = self.rng.gen::<u32>();
            Some(MapCmd::Insert(key, value))
        }
    }
}

struct List<'a, M> {
    current: M,
    prev: Option<&'a List<'a, M>>,
}

// fn do_cmds<'a, M>(mut map_list: List<'a, M>, cmds: &mut impl Iterator<Item = MapCmd<u8>>)
// where
//     M: Map<'a, u8, u32>,
type M<'a> = (ScopedMap<'a, u8, u32>, Spec<u8, u32>);
fn do_cmds<'a>(mut map_list: List<'a, M<'a>>, cmds: &mut impl Iterator<Item = MapCmd<u8>>) {
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

#[test]
fn random_test() {
    #[cfg(not(miri))]
    const ITERS: usize = 10_000_000;
    #[cfg(miri)]
    const ITERS: usize = 1_000;

    let mut iter = RandCmds::new().take(ITERS);
    let map_base = ScopedMapBase::new();
    let map = map_base.make_map();
    let spec = Spec::new();
    do_cmds(
        List {
            current: (map, spec),
            prev: None,
        },
        &mut iter,
    );
}

#[test]
fn simple_test() {
    let base = ScopedMapBase::new();
    let mut map = base.make_map();
    map.insert('a', "apple");
    map.insert('b', "banana");
    assert_eq!(map.lookup(&'a'), Some(&"apple"));
    assert_eq!(map.lookup(&'b'), Some(&"banana"));
    {
        let mut sub_map = map.new_scope();
        sub_map.insert('c', "citrus? idk");

        assert_eq!(sub_map.lookup(&'a'), Some(&"apple"));
        assert_eq!(sub_map.lookup(&'b'), Some(&"banana"));
        assert_eq!(sub_map.lookup(&'c'), Some(&"citrus? idk"));

        assert_eq!(map.lookup(&'a'), Some(&"apple"));
        assert_eq!(map.lookup(&'b'), Some(&"banana"));
        assert_eq!(map.lookup(&'c'), None);
    }
    assert_eq!(map.lookup(&'a'), Some(&"apple"));
    assert_eq!(map.lookup(&'b'), Some(&"banana"));
    assert_eq!(map.lookup(&'c'), None);
}

fn insert_from(xs: &mut dyn Iterator<Item = char>, mut map: ScopedMap<char, u32>) {
    let key = match xs.next() {
        Some(next) => next,
        _ => return,
    };
    let uniq_val = map.generation;
    map.insert(key, uniq_val);
    assert_eq!(map.lookup(&key), Some(&uniq_val));
    insert_from(xs, map.new_scope());
    assert_eq!(map.lookup(&key), Some(&uniq_val));
}

#[test]
fn another_simple_test() {
    let base = ScopedMapBase::new();
    insert_from(
        &mut "hello there, this has repeated characters in it".chars(),
        base.make_map(),
    );
}
