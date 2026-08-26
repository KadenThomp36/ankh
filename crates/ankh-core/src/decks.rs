use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct DeckId(pub i64);

/// One deck in the tree, with due counts already capped by daily limits
/// (what Anki shows on its main screen).
#[derive(Debug, Clone, Serialize)]
pub struct DeckNode {
    pub id: DeckId,
    /// Leaf name (`Vocabulary`), not the full `Korean::Vocabulary`.
    pub name: String,
    pub full_name: String,
    pub level: u32,
    pub collapsed: bool,
    pub filtered: bool,
    pub new: u32,
    pub learn: u32,
    pub review: u32,
    /// Cards in this deck only.
    pub total: u32,
    /// Cards including subdecks.
    pub total_with_children: u32,
    pub children: Vec<DeckNode>,
}

impl DeckNode {
    pub fn due(&self) -> u32 {
        self.new + self.learn + self.review
    }

    /// Depth-first walk, skipping the children of collapsed decks — exactly
    /// what a tree view renders.
    pub fn visible<'a>(&'a self, out: &mut Vec<&'a DeckNode>) {
        out.push(self);
        if !self.collapsed {
            for c in &self.children {
                c.visible(out);
            }
        }
    }

    pub fn walk<'a>(&'a self, out: &mut Vec<&'a DeckNode>) {
        out.push(self);
        for c in &self.children {
            c.walk(out);
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckTree {
    pub roots: Vec<DeckNode>,
}

impl DeckTree {
    pub(crate) fn from_proto(root: anki_proto::decks::DeckTreeNode) -> Self {
        fn conv(n: anki_proto::decks::DeckTreeNode, parent_full: &str) -> DeckNode {
            let full_name = if parent_full.is_empty() { n.name.clone() } else { format!("{parent_full}::{}", n.name) };
            DeckNode {
                id: DeckId(n.deck_id),
                name: n.name,
                level: n.level,
                collapsed: n.collapsed,
                filtered: n.filtered,
                new: n.new_count,
                learn: n.learn_count,
                review: n.review_count,
                total: n.total_in_deck,
                total_with_children: n.total_including_children,
                children: n.children.into_iter().map(|c| conv(c, &full_name)).collect(),
                full_name,
            }
        }
        DeckTree { roots: root.children.into_iter().map(|c| conv(c, "")).collect() }
    }

    pub fn visible(&self) -> Vec<&DeckNode> {
        let mut out = Vec::new();
        for r in &self.roots {
            r.visible(&mut out);
        }
        out
    }

    pub fn all(&self) -> Vec<&DeckNode> {
        let mut out = Vec::new();
        for r in &self.roots {
            r.walk(&mut out);
        }
        out
    }

    pub fn totals(&self) -> (u32, u32, u32) {
        self.roots.iter().fold((0, 0, 0), |(n, l, r), d| (n + d.new, l + d.learn, r + d.review))
    }
}
