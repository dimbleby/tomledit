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
    /// `revised_at > revision`, the proxy is stale.
    pub(crate) fn is_valid(&self, path: &[Key], revision: u64) -> bool {
        let mut node = &self.root;
        let mut keys = path.iter();
        loop {
            if node.revised_at > revision {
                return false;
            }
            match keys.next().and_then(|k| node.children.get(k)) {
                Some(child) => node = child,
                None => return true,
            }
        }
    }

    /// Stamp the node at `path` with `revision`, marking it as revised.
    /// Creates intermediate nodes as needed (with `revised_at` 0).
    /// Any children below the target are pruned — the stamped node's revision
    /// already invalidates all descendant proxies.
    pub(crate) fn stamp(&mut self, path: &[Key], revision: u64) {
        let mut node = &mut self.root;
        for key in path {
            node = node
                .children
                .entry(key.clone())
                .or_insert_with(TrieNode::new);
        }
        node.revised_at = revision;
        node.children.clear();
    }
}

impl TrieNode {
    fn new() -> Self {
        Self {
            revised_at: 0,
            children: HashMap::new(),
        }
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
}
