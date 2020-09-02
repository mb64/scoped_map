use crate::*;

#[test]
fn test() {
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
