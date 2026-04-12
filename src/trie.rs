use std::collections::HashMap;

use crate::item_ops::Key;

/// A trie that records where mutations have occurred in a TOML
/// document. Each node carries a revision timestamp. Proxies check ancestor
/// revisions along their path to detect staleness — if any ancestor was
/// mutated after the proxy was created, the proxy is stale.
///
/// The trie owns the document's monotonic revision counter. Every stamping
/// operation increments it and records the new value at the target node.
/// This ensures the revision and trie are always consistent (both updated
/// under the same `RwLock` write guard).
///
/// The trie is lazily populated: nodes are created only by `stamp` when a
/// mutation is recorded. `is_valid` never creates nodes.
pub(crate) struct MutationTrie {
    root: TrieNode,
    revision: u64,
}

#[derive(Default)]
struct TrieNode {
    /// Set to the revision at the time this node was last mutated. 0 = never.
    revised_at: u64,
    children: HashMap<Key, TrieNode>,
}

impl MutationTrie {
    pub(crate) fn new() -> Self {
        Self {
            root: TrieNode::default(),
            revision: 0,
        }
    }

    /// The current document revision.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// Check whether a proxy at `path` created at `revision` is still valid.
    ///
    /// Walks the trie from root along `path`. If any node along the way has
    /// `revised_at > revision`, the proxy is stale.
    pub(crate) fn is_valid(&self, path: &[Key], revision: u64) -> bool {
        let mut node = &self.root;
        let mut keys = path.iter();
        loop {
            if node.revised_at > revision {
                return false;
            }
            let Some(key) = keys.next() else {
                return true;
            };
            match node.children.get(key) {
                Some(child) => node = child,
                None => return true,
            }
        }
    }

    /// Record a mutation at `path`. Increments the revision and stamps the
    /// target node. Returns the new revision.
    ///
    /// Creates intermediate nodes as needed (with `revised_at` 0).
    /// Any children below the target are pruned — the stamped node's revision
    /// already invalidates all descendant proxies.
    pub(crate) fn stamp(&mut self, path: &[Key]) -> u64 {
        self.revision += 1;
        let node = self.root.walk(path);
        node.revised_at = self.revision;
        node.children.clear();
        self.revision
    }

    /// Like `stamp`, but appends one extra key segment without cloning the
    /// base path into a temporary Vec. Returns the new revision.
    pub(crate) fn stamp_child(&mut self, path: &[Key], child: &Key) -> u64 {
        self.revision += 1;
        let parent = self.root.walk(path);
        let child_node = parent.children.entry(child.clone()).or_default();
        child_node.revised_at = self.revision;
        child_node.children.clear();
        self.revision
    }

    /// Stamp each index in `from..to` as a child of `path`, sharing one
    /// revision increment. Returns the new revision.
    pub(crate) fn stamp_range(&mut self, path: &[Key], from: usize, to: usize) -> u64 {
        self.revision += 1;
        let parent = self.root.walk(path);
        for i in from..to {
            let child_node = parent.children.entry(Key::Int(i)).or_default();
            child_node.revised_at = self.revision;
            child_node.children.clear();
        }
        self.revision
    }
}

impl TrieNode {
    /// Walk to the node at `path`, creating intermediates as needed.
    fn walk(&mut self, path: &[Key]) -> &mut Self {
        let mut node = self;
        for key in path {
            node = node.children.entry(key.clone()).or_default();
        }
        node
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn str_key(s: &str) -> Key {
        Key::Str(s.to_owned())
    }
    fn int_key(i: usize) -> Key {
        Key::Int(i)
    }

    #[test]
    fn fresh_trie_always_valid() {
        let trie = MutationTrie::new();
        assert!(trie.is_valid(&[], 0));
        assert!(trie.is_valid(&[str_key("x")], 0));
        assert!(trie.is_valid(&[str_key("a"), str_key("b"), str_key("c")], 0));
    }

    #[test]
    fn stamp_leaf_invalidates_that_path() {
        let mut trie = MutationTrie::new();
        trie.stamp(&[str_key("x")]);
        assert!(!trie.is_valid(&[str_key("x")], 0));
    }

    #[test]
    fn stamp_leaf_does_not_affect_sibling() {
        let mut trie = MutationTrie::new();
        trie.stamp(&[str_key("x")]);
        assert!(trie.is_valid(&[str_key("y")], 0));
    }

    #[test]
    fn stamp_parent_invalidates_descendant() {
        let mut trie = MutationTrie::new();
        trie.stamp(&[str_key("t")]);
        // Proxy at ["t", "a"] checks ["t"] → stamped → stale
        assert!(!trie.is_valid(&[str_key("t"), str_key("a")], 0));
    }

    #[test]
    fn stamp_child_does_not_invalidate_parent() {
        let mut trie = MutationTrie::new();
        trie.stamp(&[str_key("t"), str_key("a")]);
        // Proxy at ["t"] should still be valid
        assert!(trie.is_valid(&[str_key("t")], 0));
    }

    #[test]
    fn stamp_child_does_not_invalidate_sibling() {
        let mut trie = MutationTrie::new();
        trie.stamp(&[str_key("t"), str_key("a")]);
        assert!(trie.is_valid(&[str_key("t"), str_key("b")], 0));
    }

    #[test]
    fn stamp_root_invalidates_everything() {
        let mut trie = MutationTrie::new();
        trie.stamp(&[]);
        assert!(!trie.is_valid(&[], 0));
        assert!(!trie.is_valid(&[str_key("x")], 0));
        assert!(!trie.is_valid(&[str_key("a"), str_key("b")], 0));
    }

    #[test]
    fn proxy_created_after_mutation_is_valid() {
        let mut trie = MutationTrie::new();
        let rev = trie.stamp(&[str_key("x")]);
        assert!(trie.is_valid(&[str_key("x")], rev));
    }

    #[test]
    fn self_update_keeps_proxy_valid() {
        let mut trie = MutationTrie::new();
        // Proxy at ["arr"] does insert → stamps self
        let rev = trie.stamp(&[str_key("arr")]);
        assert!(trie.is_valid(&[str_key("arr")], rev));
        // But element proxy at ["arr", 0] with old revision is stale
        assert!(!trie.is_valid(&[str_key("arr"), int_key(0)], 0));
    }

    #[test]
    fn later_ancestor_stamp_invalidates_self_updated_proxy() {
        let mut trie = MutationTrie::new();
        // Self-update after own stamp
        let rev = trie.stamp(&[str_key("t")]);
        assert!(trie.is_valid(&[str_key("t")], rev));
        // Now root is stamped (doc.clear())
        trie.stamp(&[]);
        assert!(!trie.is_valid(&[str_key("t")], rev));
    }

    #[test]
    fn deep_path_only_affected_by_ancestors() {
        let mut trie = MutationTrie::new();
        // Stamp a completely unrelated deep path
        trie.stamp(&[str_key("a"), str_key("b"), str_key("c")]);
        // Unrelated paths still valid
        assert!(trie.is_valid(&[str_key("x"), str_key("y")], 0));
        assert!(trie.is_valid(&[str_key("a"), str_key("d")], 0));
        // Same path is stale
        assert!(!trie.is_valid(&[str_key("a"), str_key("b"), str_key("c")], 0));
        // Deeper path under same prefix is also stale (ancestor stamped)
        assert!(!trie.is_valid(&[str_key("a"), str_key("b"), str_key("c"), str_key("d")], 0));
    }

    #[test]
    fn multiple_stamps_tracked_independently() {
        let mut trie = MutationTrie::new();
        let rev_x = trie.stamp(&[str_key("x")]);
        let rev_y = trie.stamp(&[str_key("y")]);

        // Revision 0 proxy: x is stale, y is stale
        assert!(!trie.is_valid(&[str_key("x")], 0));
        assert!(!trie.is_valid(&[str_key("y")], 0));

        // Proxy created after x stamp: x is valid, y is stale
        assert!(trie.is_valid(&[str_key("x")], rev_x));
        assert!(!trie.is_valid(&[str_key("y")], rev_x));

        // Proxy created after y stamp: both valid
        assert!(trie.is_valid(&[str_key("x")], rev_y));
        assert!(trie.is_valid(&[str_key("y")], rev_y));
    }

    #[test]
    fn int_keys_work() {
        let mut trie = MutationTrie::new();
        trie.stamp(&[str_key("arr"), int_key(2)]);
        assert!(!trie.is_valid(&[str_key("arr"), int_key(2)], 0));
        assert!(trie.is_valid(&[str_key("arr"), int_key(0)], 0));
        assert!(trie.is_valid(&[str_key("arr")], 0));
    }

    #[test]
    fn array_structural_stamp_invalidates_all_elements() {
        let mut trie = MutationTrie::new();
        // Array insert → stamp the array node itself
        trie.stamp(&[str_key("arr")]);
        assert!(!trie.is_valid(&[str_key("arr"), int_key(0)], 0));
        assert!(!trie.is_valid(&[str_key("arr"), int_key(1)], 0));
        assert!(!trie.is_valid(&[str_key("arr"), int_key(99)], 0));
    }

    #[test]
    fn stamp_child_equivalent_to_stamp() {
        let mut trie1 = MutationTrie::new();
        let rev1 = trie1.stamp(&[str_key("arr"), int_key(2)]);

        let mut trie2 = MutationTrie::new();
        let rev2 = trie2.stamp_child(&[str_key("arr")], &int_key(2));

        assert_eq!(rev1, rev2);

        // Both should produce identical validity results
        for rev in [0, rev1] {
            assert_eq!(
                trie1.is_valid(&[str_key("arr"), int_key(2)], rev),
                trie2.is_valid(&[str_key("arr"), int_key(2)], rev),
            );
            assert_eq!(
                trie1.is_valid(&[str_key("arr"), int_key(0)], rev),
                trie2.is_valid(&[str_key("arr"), int_key(0)], rev),
            );
            assert_eq!(
                trie1.is_valid(&[str_key("arr")], rev),
                trie2.is_valid(&[str_key("arr")], rev),
            );
        }
    }

    #[test]
    fn stamp_child_reuses_existing_node() {
        let mut trie = MutationTrie::new();
        // First stamp creates the node
        trie.stamp_child(&[str_key("t")], &str_key("a"));
        assert!(!trie.is_valid(&[str_key("t"), str_key("a")], 0));
        // Second stamp reuses it (no extra allocation)
        let rev = trie.stamp_child(&[str_key("t")], &str_key("a"));
        assert!(!trie.is_valid(&[str_key("t"), str_key("a")], 1));
        assert!(trie.is_valid(&[str_key("t"), str_key("a")], rev));
        // Sibling unaffected
        assert!(trie.is_valid(&[str_key("t"), str_key("b")], 0));
    }

    // -- per-element range stamping tests --

    #[test]
    fn range_invalidates_stamped_indices() {
        let mut trie = MutationTrie::new();
        let arr = [str_key("arr")];
        let rev = trie.stamp_range(&arr, 2, 5);
        // Index 0, 1: not stamped → valid
        assert!(trie.is_valid(&[str_key("arr"), int_key(0)], 0));
        assert!(trie.is_valid(&[str_key("arr"), int_key(1)], 0));
        // Index 2, 3, 4: stamped → stale
        assert!(!trie.is_valid(&[str_key("arr"), int_key(2)], 0));
        assert!(!trie.is_valid(&[str_key("arr"), int_key(3)], 0));
        assert!(!trie.is_valid(&[str_key("arr"), int_key(4)], 0));
        // Index 5: not stamped → valid
        assert!(trie.is_valid(&[str_key("arr"), int_key(5)], 0));
        // Array node itself: still valid
        assert!(trie.is_valid(&arr, 0));
        // Proxy created at the stamp revision: valid
        assert!(trie.is_valid(&[str_key("arr"), int_key(3)], rev));
    }

    #[test]
    fn stamp_clears_children() {
        let mut trie = MutationTrie::new();
        let arr = [str_key("arr")];
        let range_rev = trie.stamp_range(&arr, 0, 3);
        // All elements stale
        assert!(!trie.is_valid(&[str_key("arr"), int_key(0)], 0));
        // Structural stamp subsumes the children
        let struct_rev = trie.stamp(&arr);
        // Proxy created at range revision (before structural stamp): stale
        assert!(!trie.is_valid(&[str_key("arr"), int_key(0)], range_rev));
        // Proxy created at structural stamp revision: valid
        assert!(trie.is_valid(&[str_key("arr"), int_key(0)], struct_rev));
    }

    #[test]
    fn non_overlapping_ranges_preserve_between() {
        // Two disjoint range stamps should not interfere with each other.
        let mut trie = MutationTrie::new();
        let arr = [str_key("arr")];
        // First range: indices 1..8 (simulates pop(1) on 8-element array)
        let rev1 = trie.stamp_range(&arr, 1, 8);
        // Proxy at index 3 created after first range
        // Second range: indices 5..7 (simulates pop(5) on 7-element array)
        trie.stamp_range(&arr, 5, 7);
        // Index 3 was stamped at rev1 but proxy was created at rev1 → valid
        assert!(trie.is_valid(&[str_key("arr"), int_key(3)], rev1));
        // Index 5 was re-stamped → stale for proxy at rev1
        assert!(!trie.is_valid(&[str_key("arr"), int_key(5)], rev1));
    }

    #[test]
    fn range_interleaved_with_child_stamp() {
        let mut trie = MutationTrie::new();
        let arr = [str_key("arr")];
        // Replace arr[3] in place
        let rev1 = trie.stamp_child(&arr, &int_key(3));
        // Delete at index 1 → range stamp indices 1..5
        trie.stamp_range(&arr, 1, 5);
        // Proxy at arr[3] created at rev1: re-stamped → stale
        assert!(!trie.is_valid(&[str_key("arr"), int_key(3)], rev1));
        // Proxy at arr[0] created at rev 0: not in range → valid
        assert!(trie.is_valid(&[str_key("arr"), int_key(0)], 0));
    }

    #[test]
    fn revision_increments_monotonically() {
        let mut trie = MutationTrie::new();
        assert_eq!(trie.revision(), 0);
        assert_eq!(trie.stamp(&[str_key("a")]), 1);
        assert_eq!(trie.stamp_child(&[str_key("b")], &str_key("c")), 2);
        assert_eq!(trie.stamp_range(&[str_key("d")], 0, 3), 3);
        assert_eq!(trie.revision(), 3);
    }
}
