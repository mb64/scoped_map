use crate::*;

#[macro_use]
mod infra;
use self::infra::*;

use rand::distributions::Standard;

// Make sure it's covariant in lifetime
fn _variance_check<'a>(map: &'a ScopedMap<'static, u8, u8>) {
    let _other_map: &'a ScopedMap<'a, u8, u8> = map;
}

random_test! {
    name: spec_u8;
    item: u8;
    map: (ScopedMap<'a, u8, u32>, Spec<u8, u32>);
    init: |x| (x, Spec::new());
    normally: 5_000_000;
    miri: 100;
}

random_test! {
    name: fuzz_u8_miri;
    item: u8;
    map: ScopedMap<'a, u8, u32>;
    init: |x| x;
    normally: 10_000;
    miri: 10_000;
}

random_test! {
    name: spec_collide;
    item: BadHash;
    map: (ScopedMap<'a, BadHash, u32>, Spec<BadHash, u32>);
    init: |x| (x, Spec::new());
    normally: 5_000_000;
    miri: 100;
}

random_test! {
    name: fuzz_collide_miri;
    item: BadHash;
    map: ScopedMap<'a, BadHash, u32>;
    init: |x| x;
    normally: 1_000;
    miri: 1_000;
}

random_bench! {
    name: bench_10000_u8 "ScopedMap 10,000 u8";
    item: u8;
    map: ScopedMap<'a, u8, u32>;
    init: |x| x;
    iters: 10_000;
}

random_bench! {
    name: bench_spec_10000_u8 "Spec 10,000 u8";
    item: u8;
    map: Spec<u8, u32>;
    init: |_| Spec::new();
    iters: 10_000;
}

random_bench! {
    name: bench_im_10000_u8 "im_rc::HashMap 10,000 u8";
    item: u8;
    map: im_rc::HashMap<u8, u32, ahash::RandomState>;
    init: |_| im_rc::HashMap::with_hasher(ahash::RandomState::new());
    iters: 10_000;
}

random_bench! {
    name: bench_100000_u8 "ScopedMap 100,000 u8";
    item: u8;
    map: ScopedMap<'a, u8, u32>;
    init: |x| x;
    iters: 100_000;
}

random_bench! {
    name: bench_10000_collide "ScopedMap 10,000 BadHash";
    item: BadHash;
    map: ScopedMap<'a, BadHash, u32>;
    init: |x| x;
    iters: 10_000;
}

random_bench! {
    name: bench_im_10000_collide "im_rc::HashMap 10,000 BadHash";
    item: BadHash;
    map: im_rc::HashMap<BadHash, u32, ahash::RandomState>;
    init: |_| im_rc::HashMap::with_hasher(ahash::RandomState::new());
    iters: 10_000;
}

random_bench! {
    name: bench_spec_10000_collide "Spec 10,000 BadHash";
    item: BadHash;
    map: Spec<BadHash, u32>;
    init: |_| Spec::new();
    iters: 10_000;
}

random_bench! {
    name: bench_100000_collide "ScopedMap 100,000 BadHash";
    item: BadHash;
    map: ScopedMap<'a, BadHash, u32>;
    init: |x| x;
    iters: 100_000;
}

random_bench_zipf! {
    name: zipf_scoped "ScopedMap Zipf 100k";
    map: ScopedMap<'a, &'static str, u32>;
    init: |x| x;
    iters: 100_000;
    zipf count: 100_000;
}
random_bench_zipf! {
    name: zipf_spec "Spec Zipf 100k";
    map: Spec<&'static str, u32>;
    init: |_| Spec::new();
    iters: 100_000;
    zipf count: 100_000;
}
random_bench_zipf! {
    name: zipf_im "im_rc::HashMap Zipf 100k";
    map: im_rc::HashMap<&'static str, u32>;
    init: |_| im_rc::HashMap::new();
    iters: 100_000;
    zipf count: 100_000;
}

#[cfg(not(feature = "benching"))]
mod handwritten {
    use super::*;

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

    /// Tests variance of the map, and that you can store references in it which get invalidated
    /// after the scope
    #[test]
    fn arena_test() {
        use typed_arena::{Arena, SubArena};
        let my_arena = Arena::new();
        let base = ScopedMapBase::<i32, &'static mut str>::new();
        let mut map = base.make_map();
        map.insert(1, my_arena.alloc_str("one"));
        map.insert(2, my_arena.alloc_str("two"));
        {
            let sub_arena = SubArena::new(&my_arena);
            let mut sub_map = map.new_scope();
            sub_map.insert(5, sub_arena.alloc_str("five"));
            sub_map.insert(1, sub_arena.alloc_str("One!"));
            assert_eq!(sub_map.lookup(&2).map(|x| &**x), Some("two"));
            assert_eq!(sub_map.lookup(&1).map(|x| &**x), Some("One!"));
            assert_eq!(map.lookup(&1).map(|x| &**x), Some("one"));
        }
        assert_eq!(map.lookup(&1).map(|x| &**x), Some("one"));
        assert_eq!(map.lookup(&5), None);
    }

    #[test]
    fn another_simple_test() {
        let base = ScopedMapBase::new();
        insert_from(
            &mut "hello there, this has repeated characters in it".chars(),
            base.make_map(),
        );
    }
}
