use std::collections::HashMap;

use crate::item_ops::Key;

/// A trie that records where mutations have occurred in a TOML
/// document. Each node carries a revision timestamp. Proxies check ancestor
/// revisions along their path to detect staleness — if any ancestor was
/// mutated after the proxy was created, the proxy is stale.
///
/// The trie is lazily populated: nodes are created only by `stamp` when a
/// mutation is recorded. `is_valid` never creates nodes.
pub(crate) struct MutationTrie {
    root: TrieNode,
}

struct TrieNode {
    /// Set to the revision at the time this node was last mutated. 0 = never.
    revised_at: u64,
    /// When an array mutation shifts indices, records the lowest affected
    /// index and the revision.  `is_valid` treats `Key::Int(i)` where
    /// `i >= threshold` as stale if the revision is newer than the proxy's.
    /// Multiple shifts accumulate by taking `min(threshold)`.
    shifted_from: Option<(usize, u64)>,
    children: HashMap<Key, TrieNode>,
}

impl MutationTrie {
    pub(crate) fn new() -> Self {
        Self {
            root: TrieNode::new(),
        }
    }

    /// Check whether a proxy at `path` created at `revision` is still valid.
    ///
    /// Walks the trie from root along `path`. If any node along the way has
    /// `revised_at > revision`, the proxy is stale. Additionally, for integer
    /// keys, checks `shifted_from` — if the index is at or beyond the shift
    /// threshold and the shift revision is newer, the proxy is stale.
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
            if let Key::Int(i) = key
                && let Some((from, rev)) = node.shifted_from
                && *i >= from
                && rev > revision
            {
                return false;
            }
            match node.children.get(key) {
                Some(child) => node = child,
                None => return true,
            }
        }
    }

    /// Stamp the node at `path` with `revision`, marking it as revised.
    /// Creates intermediate nodes as needed (with `revised_at` 0).
    /// Any children below the target are pruned — the stamped node's revision
    /// already invalidates all descendant proxies. Also clears `shifted_from`
    /// since `revised_at` subsumes it.
    pub(crate) fn stamp(&mut self, path: &[Key], revision: u64) {
        let node = self.root.walk(path);
        node.revised_at = revision;
        node.shifted_from = None;
        node.children.clear();
    }

    /// Like `stamp`, but appends one extra key segment without cloning the
    /// base path into a temporary Vec.
    pub(crate) fn stamp_child(&mut self, path: &[Key], child: &Key, revision: u64) {
        let parent = self.root.walk(path);
        let child_node = parent
            .children
            .entry(child.clone())
            .or_insert_with(TrieNode::new);
        child_node.revised_at = revision;
        child_node.children.clear();
    }

    /// Record that an array mutation shifted indices starting at `from_index`.
    /// Accumulates with any prior shift by taking the minimum threshold.
    pub(crate) fn stamp_shift(&mut self, path: &[Key], from_index: usize, revision: u64) {
        let node = self.root.walk(path);
        node.shifted_from = Some(match node.shifted_from {
            Some((existing_from, _)) => (existing_from.min(from_index), revision),
            None => (from_index, revision),
        });
    }
}

impl TrieNode {
    fn new() -> Self {
        Self {
            revised_at: 0,
            shifted_from: None,
            children: HashMap::new(),
        }
    }

    /// Walk to the node at `path`, creating intermediates as needed.
    fn walk(&mut self, path: &[Key]) -> &mut Self {
        let mut node = self;
        for key in path {
            node = node
                .children
                .entry(key.clone())
                .or_insert_with(TrieNode::new);
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
        trie.stamp(&[str_key("x")], 1);
        assert!(!trie.is_valid(&[str_key("x")], 0));
    }

    #[test]
    fn stamp_leaf_does_not_affect_sibling() {
        let mut trie = MutationTrie::new();
        trie.stamp(&[str_key("x")], 1);
        assert!(trie.is_valid(&[str_key("y")], 0));
    }

    #[test]
    fn stamp_parent_invalidates_descendant() {
        let mut trie = MutationTrie::new();
        trie.stamp(&[str_key("t")], 1);
        // Proxy at ["t", "a"] checks ["t"] → stamped → stale
        assert!(!trie.is_valid(&[str_key("t"), str_key("a")], 0));
    }

    #[test]
    fn stamp_child_does_not_invalidate_parent() {
        let mut trie = MutationTrie::new();
        trie.stamp(&[str_key("t"), str_key("a")], 1);
        // Proxy at ["t"] should still be valid
        assert!(trie.is_valid(&[str_key("t")], 0));
    }

    #[test]
    fn stamp_child_does_not_invalidate_sibling() {
        let mut trie = MutationTrie::new();
        trie.stamp(&[str_key("t"), str_key("a")], 1);
        assert!(trie.is_valid(&[str_key("t"), str_key("b")], 0));
    }

    #[test]
    fn stamp_root_invalidates_everything() {
        let mut trie = MutationTrie::new();
        trie.stamp(&[], 1);
        assert!(!trie.is_valid(&[], 0));
        assert!(!trie.is_valid(&[str_key("x")], 0));
        assert!(!trie.is_valid(&[str_key("a"), str_key("b")], 0));
    }

    #[test]
    fn proxy_created_after_mutation_is_valid() {
        let mut trie = MutationTrie::new();
        trie.stamp(&[str_key("x")], 1);
        assert!(trie.is_valid(&[str_key("x")], 1));
    }

    #[test]
    fn self_update_keeps_proxy_valid() {
        let mut trie = MutationTrie::new();
        // Proxy at ["arr"] does insert → stamps self
        trie.stamp(&[str_key("arr")], 1);
        assert!(trie.is_valid(&[str_key("arr")], 1));
        // But element proxy at ["arr", 0] with old revision is stale
        assert!(!trie.is_valid(&[str_key("arr"), int_key(0)], 0));
    }

    #[test]
    fn later_ancestor_stamp_invalidates_self_updated_proxy() {
        let mut trie = MutationTrie::new();
        // Self-update after own stamp
        trie.stamp(&[str_key("t")], 1);
        assert!(trie.is_valid(&[str_key("t")], 1));
        // Now root is stamped (doc.clear())
        trie.stamp(&[], 2);
        assert!(!trie.is_valid(&[str_key("t")], 1));
    }

    #[test]
    fn deep_path_only_affected_by_ancestors() {
        let mut trie = MutationTrie::new();
        // Stamp a completely unrelated deep path
        trie.stamp(&[str_key("a"), str_key("b"), str_key("c")], 1);
        // Unrelated paths still valid
        assert!(trie.is_valid(&[str_key("x"), str_key("y")], 0));
        assert!(trie.is_valid(&[str_key("a"), str_key("d")], 0));
        // Same path is stale
        assert!(!trie.is_valid(&[str_key("a"), str_key("b"), str_key("c")], 0));
        // Deeper path under same prefix is also stale (ancestor stamped)
        // We stamped ["a","b","c"], not ["a","b"]. So ["a","b","c","d"] checks
        // root(0), a(0), b(0), c(stamped at 1) → c > 0 → stale
        assert!(!trie.is_valid(&[str_key("a"), str_key("b"), str_key("c"), str_key("d")], 0));
    }

    #[test]
    fn multiple_stamps_tracked_independently() {
        let mut trie = MutationTrie::new();
        trie.stamp(&[str_key("x")], 1);
        trie.stamp(&[str_key("y")], 2);

        // Revision 0 proxy: x is stale, y is stale
        assert!(!trie.is_valid(&[str_key("x")], 0));
        assert!(!trie.is_valid(&[str_key("y")], 0));

        // Revision 1 proxy (created after x stamp): x is valid, y is stale
        assert!(trie.is_valid(&[str_key("x")], 1));
        assert!(!trie.is_valid(&[str_key("y")], 1));

        // Revision 2 proxy: both valid
        assert!(trie.is_valid(&[str_key("x")], 2));
        assert!(trie.is_valid(&[str_key("y")], 2));
    }

    #[test]
    fn int_keys_work() {
        let mut trie = MutationTrie::new();
        trie.stamp(&[str_key("arr"), int_key(2)], 1);
        assert!(!trie.is_valid(&[str_key("arr"), int_key(2)], 0));
        assert!(trie.is_valid(&[str_key("arr"), int_key(0)], 0));
        assert!(trie.is_valid(&[str_key("arr")], 0));
    }

    #[test]
    fn array_structural_stamp_invalidates_all_elements() {
        let mut trie = MutationTrie::new();
        // Array insert → stamp the array node itself
        trie.stamp(&[str_key("arr")], 1);
        assert!(!trie.is_valid(&[str_key("arr"), int_key(0)], 0));
        assert!(!trie.is_valid(&[str_key("arr"), int_key(1)], 0));
        assert!(!trie.is_valid(&[str_key("arr"), int_key(99)], 0));
    }

    #[test]
    fn stamp_child_equivalent_to_stamp() {
        let mut trie1 = MutationTrie::new();
        trie1.stamp(&[str_key("arr"), int_key(2)], 1);

        let mut trie2 = MutationTrie::new();
        trie2.stamp_child(&[str_key("arr")], &int_key(2), 1);

        // Both should produce identical validity results
        for rev in [0, 1] {
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
        trie.stamp_child(&[str_key("t")], &str_key("a"), 1);
        assert!(!trie.is_valid(&[str_key("t"), str_key("a")], 0));
        // Second stamp reuses it (no extra allocation)
        trie.stamp_child(&[str_key("t")], &str_key("a"), 2);
        assert!(!trie.is_valid(&[str_key("t"), str_key("a")], 1));
        assert!(trie.is_valid(&[str_key("t"), str_key("a")], 2));
        // Sibling unaffected
        assert!(trie.is_valid(&[str_key("t"), str_key("b")], 0));
    }

    // -- stamp_shift tests --

    #[test]
    fn shift_invalidates_indices_at_and_after_threshold() {
        let mut trie = MutationTrie::new();
        let arr = [str_key("arr")];
        trie.stamp_shift(&arr, 2, 1);
        // Index 0, 1: below threshold → valid
        assert!(trie.is_valid(&[str_key("arr"), int_key(0)], 0));
        assert!(trie.is_valid(&[str_key("arr"), int_key(1)], 0));
        // Index 2, 3: at/above threshold → stale
        assert!(!trie.is_valid(&[str_key("arr"), int_key(2)], 0));
        assert!(!trie.is_valid(&[str_key("arr"), int_key(3)], 0));
        // Array node itself: still valid
        assert!(trie.is_valid(&arr, 0));
    }

    #[test]
    fn shift_does_not_affect_string_keys() {
        let mut trie = MutationTrie::new();
        trie.stamp_shift(&[str_key("t")], 0, 1);
        // String key children are unaffected by shift
        assert!(trie.is_valid(&[str_key("t"), str_key("a")], 0));
    }

    #[test]
    fn shift_accumulates_with_min_threshold() {
        let mut trie = MutationTrie::new();
        let arr = [str_key("arr")];
        trie.stamp_shift(&arr, 5, 1);
        trie.stamp_shift(&arr, 2, 2);
        // Threshold is min(5, 2) = 2
        assert!(trie.is_valid(&[str_key("arr"), int_key(1)], 0));
        assert!(!trie.is_valid(&[str_key("arr"), int_key(2)], 0));
    }

    #[test]
    fn shift_proxy_created_after_shift_is_valid() {
        let mut trie = MutationTrie::new();
        trie.stamp_shift(&[str_key("arr")], 2, 1);
        // Proxy created at revision 1 (after the shift): valid
        assert!(trie.is_valid(&[str_key("arr"), int_key(3)], 1));
    }

    #[test]
    fn stamp_clears_shifted_from() {
        let mut trie = MutationTrie::new();
        let arr = [str_key("arr")];
        trie.stamp_shift(&arr, 0, 1);
        // All elements stale
        assert!(!trie.is_valid(&[str_key("arr"), int_key(0)], 0));
        // Structural stamp subsumes the shift
        trie.stamp(&arr, 2);
        // Proxy created at revision 1 (after shift, before stamp):
        // stamp set revised_at=2 on the node, so still stale via revised_at
        assert!(!trie.is_valid(&[str_key("arr"), int_key(0)], 1));
        // Proxy created at revision 2: valid (both revised_at and shift cleared)
        assert!(trie.is_valid(&[str_key("arr"), int_key(0)], 2));
    }

    #[test]
    fn shift_interleaved_with_child_stamp() {
        let mut trie = MutationTrie::new();
        let arr = [str_key("arr")];
        // Replace arr[3] in place
        trie.stamp_child(&arr, &int_key(3), 1);
        // Create proxy at arr[3] after replacement
        // Then delete at index 1 → shift from 1
        trie.stamp_shift(&arr, 1, 2);
        // Proxy at arr[3] created at rev 1: index 3 >= 1, rev 2 > 1 → stale
        assert!(!trie.is_valid(&[str_key("arr"), int_key(3)], 1));
        // Proxy at arr[0] created at rev 0: index 0 < 1 → valid
        assert!(trie.is_valid(&[str_key("arr"), int_key(0)], 0));
    }
}
