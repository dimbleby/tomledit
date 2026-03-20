use std::collections::HashMap;

use crate::item_ops::Key;

/// A trie that records where mutations have occurred in a TOML
/// document. Each node carries a version timestamp. Proxies check ancestor
/// versions along their path to detect staleness — if any ancestor was
/// mutated after the proxy was created, the proxy is stale.
///
/// The trie is lazily populated: nodes are created only by `bump_at` when a
/// mutation is recorded. `is_valid` never creates nodes.
pub(crate) struct MutationTrie {
    /// Monotonically increasing clock used to generate unique version stamps.
    pub(crate) clock: u64,
    root: TrieNode,
}

struct TrieNode {
    /// Set to the clock value when this node was last mutated. 0 = never.
    version: u64,
    children: HashMap<Key, TrieNode>,
}

impl MutationTrie {
    pub(crate) fn new() -> Self {
        Self {
            clock: 0,
            root: TrieNode::new(),
        }
    }

    /// Check whether a proxy at `path` created at `generation` is still valid.
    ///
    /// Walks the trie from root along `path`. If any node along the way has
    /// `version > generation`, the proxy is stale.
    pub(crate) fn is_valid(&self, path: &[Key], generation: u64) -> bool {
        if self.root.version > generation {
            return false;
        }
        let mut node = &self.root;
        for key in path {
            match node.children.get(key) {
                Some(child) => {
                    if child.version > generation {
                        return false;
                    }
                    node = child;
                }
                None => break,
            }
        }
        true
    }

    /// Record a mutation at the given path. Creates intermediate nodes as
    /// needed (with version 0) and sets the target node's version to a new
    /// clock tick.  Any children below the target are pruned — the bumped
    /// node's version already invalidates all descendant proxies.
    pub(crate) fn bump_at(&mut self, path: &[Key]) {
        self.clock += 1;
        let mut node = &mut self.root;
        for key in path {
            node = node
                .children
                .entry(key.clone())
                .or_insert_with(TrieNode::new);
        }
        node.version = self.clock;
        node.children.clear();
    }

    /// Record a mutation at the document root (used by `doc.clear()`).
    /// Prunes the entire trie — root version alone invalidates everything.
    pub(crate) fn bump_root(&mut self) {
        self.clock += 1;
        self.root = TrieNode::new();
        self.root.version = self.clock;
    }
}

impl TrieNode {
    fn new() -> Self {
        Self {
            version: 0,
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
    fn bump_at_leaf_invalidates_that_path() {
        let mut trie = MutationTrie::new();
        let created_at = trie.clock; // 0
        trie.bump_at(&[str_key("x")]);
        assert!(!trie.is_valid(&[str_key("x")], created_at));
    }

    #[test]
    fn bump_at_leaf_does_not_affect_sibling() {
        let mut trie = MutationTrie::new();
        let created_at = trie.clock;
        trie.bump_at(&[str_key("x")]);
        assert!(trie.is_valid(&[str_key("y")], created_at));
    }

    #[test]
    fn bump_parent_invalidates_descendant() {
        let mut trie = MutationTrie::new();
        let created_at = trie.clock;
        trie.bump_at(&[str_key("t")]);
        // Proxy at ["t", "a"] checks ["t"] → bumped → stale
        assert!(!trie.is_valid(&[str_key("t"), str_key("a")], created_at));
    }

    #[test]
    fn bump_child_does_not_invalidate_parent() {
        let mut trie = MutationTrie::new();
        let created_at = trie.clock;
        trie.bump_at(&[str_key("t"), str_key("a")]);
        // Proxy at ["t"] should still be valid
        assert!(trie.is_valid(&[str_key("t")], created_at));
    }

    #[test]
    fn bump_child_does_not_invalidate_sibling() {
        let mut trie = MutationTrie::new();
        let created_at = trie.clock;
        trie.bump_at(&[str_key("t"), str_key("a")]);
        assert!(trie.is_valid(&[str_key("t"), str_key("b")], created_at));
    }

    #[test]
    fn bump_root_invalidates_everything() {
        let mut trie = MutationTrie::new();
        let created_at = trie.clock;
        trie.bump_root();
        assert!(!trie.is_valid(&[], created_at));
        assert!(!trie.is_valid(&[str_key("x")], created_at));
        assert!(!trie.is_valid(&[str_key("a"), str_key("b")], created_at));
    }

    #[test]
    fn proxy_created_after_mutation_is_valid() {
        let mut trie = MutationTrie::new();
        trie.bump_at(&[str_key("x")]);
        let created_at = trie.clock; // created after the bump
        assert!(trie.is_valid(&[str_key("x")], created_at));
    }

    #[test]
    fn self_update_keeps_proxy_valid() {
        let mut trie = MutationTrie::new();
        // Proxy at ["arr"] does insert → bumps self
        trie.bump_at(&[str_key("arr")]);
        let created_at = trie.clock; // self-update
        assert!(trie.is_valid(&[str_key("arr")], created_at));
        // But element proxy at ["arr", 0] with old generation is stale
        assert!(!trie.is_valid(&[str_key("arr"), int_key(0)], 0));
    }

    #[test]
    fn later_ancestor_bump_invalidates_self_updated_proxy() {
        let mut trie = MutationTrie::new();
        // Self-update after own bump
        trie.bump_at(&[str_key("t")]);
        let created_at = trie.clock;
        assert!(trie.is_valid(&[str_key("t")], created_at));
        // Now root is bumped (doc.clear())
        trie.bump_root();
        assert!(!trie.is_valid(&[str_key("t")], created_at));
    }

    #[test]
    fn deep_path_only_affected_by_ancestors() {
        let mut trie = MutationTrie::new();
        let created_at = trie.clock;
        // Bump a completely unrelated deep path
        trie.bump_at(&[str_key("a"), str_key("b"), str_key("c")]);
        // Unrelated paths still valid
        assert!(trie.is_valid(&[str_key("x"), str_key("y")], created_at));
        assert!(trie.is_valid(&[str_key("a"), str_key("d")], created_at));
        // Same path is stale
        assert!(!trie.is_valid(&[str_key("a"), str_key("b"), str_key("c")], created_at));
        // Deeper path under same prefix is also stale (ancestor bumped)
        // Actually no — we bumped ["a","b","c"], not ["a","b"]. So ["a","b","c","d"] checks
        // root(0), a(0), b(0), c(bumped) → c > created_at → stale for path ["a","b","c","d"]
        assert!(!trie.is_valid(
            &[str_key("a"), str_key("b"), str_key("c"), str_key("d")],
            created_at
        ));
    }

    #[test]
    fn multiple_bumps_tracked_independently() {
        let mut trie = MutationTrie::new();
        let created_at0 = trie.clock;
        trie.bump_at(&[str_key("x")]);
        let created_at1 = trie.clock;
        trie.bump_at(&[str_key("y")]);
        let created_at2 = trie.clock;

        // created_at0 proxy: x is stale, y is stale
        assert!(!trie.is_valid(&[str_key("x")], created_at0));
        assert!(!trie.is_valid(&[str_key("y")], created_at0));

        // created_at1 proxy (created after x bump): x is valid, y is stale
        assert!(trie.is_valid(&[str_key("x")], created_at1));
        assert!(!trie.is_valid(&[str_key("y")], created_at1));

        // created_at2 proxy: both valid
        assert!(trie.is_valid(&[str_key("x")], created_at2));
        assert!(trie.is_valid(&[str_key("y")], created_at2));
    }

    #[test]
    fn int_keys_work() {
        let mut trie = MutationTrie::new();
        let created_at = trie.clock;
        trie.bump_at(&[str_key("arr"), int_key(2)]);
        assert!(!trie.is_valid(&[str_key("arr"), int_key(2)], created_at));
        assert!(trie.is_valid(&[str_key("arr"), int_key(0)], created_at));
        assert!(trie.is_valid(&[str_key("arr")], created_at));
    }

    #[test]
    fn array_structural_bump_invalidates_all_elements() {
        let mut trie = MutationTrie::new();
        let created_at = trie.clock;
        // Array insert → bump the array node itself
        trie.bump_at(&[str_key("arr")]);
        assert!(!trie.is_valid(&[str_key("arr"), int_key(0)], created_at));
        assert!(!trie.is_valid(&[str_key("arr"), int_key(1)], created_at));
        assert!(!trie.is_valid(&[str_key("arr"), int_key(99)], created_at));
    }
}
