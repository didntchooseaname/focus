use super::fsx::{file_name, natural_cmp, parent_dir};
use crate::{LinkOut, NodeKind, NoteId, TreeNode, TreeSnapshot};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

pub(crate) struct Item {
    pub path: Arc<Path>,
    pub kind: NodeKind,
    pub note: Option<NoteId>,
    pub link_out: Option<LinkOut>,
}

struct Child {
    name: Arc<str>,
    item: Item,
}

pub(crate) fn build(items: impl Iterator<Item = Item>, generation: u64) -> TreeSnapshot {
    let mut children: HashMap<Arc<Path>, Vec<Child>> = HashMap::new();
    let mut known_dirs: std::collections::HashSet<Arc<Path>> = std::collections::HashSet::new();
    let push = |children: &mut HashMap<Arc<Path>, Vec<Child>>, item: Item| {
        let parent: Arc<Path> = Arc::from(parent_dir(&item.path));
        children.entry(parent).or_default().push(Child {
            name: Arc::from(file_name(&item.path)),
            item,
        });
    };
    let mut pending_dirs = Vec::new();
    for item in items {
        if item.kind == NodeKind::Folder {
            known_dirs.insert(item.path.clone());
        }
        let mut p = parent_dir(&item.path);
        while !p.as_os_str().is_empty() {
            pending_dirs.push(Arc::<Path>::from(p));
            p = parent_dir(p);
        }
        push(&mut children, item);
    }
    for d in pending_dirs {
        if known_dirs.insert(d.clone()) {
            push(
                &mut children,
                Item {
                    path: d,
                    kind: NodeKind::Folder,
                    note: None,
                    link_out: None,
                },
            );
        }
    }
    use rayon::prelude::*;
    children.par_iter_mut().for_each(|(_, list)| {
        list.sort_by(|a, b| {
            let fa = a.item.kind != NodeKind::Folder;
            let fb = b.item.kind != NodeKind::Folder;
            fa.cmp(&fb).then_with(|| natural_cmp(&a.name, &b.name))
        });
    });
    let mut nodes = Vec::new();
    emit(&mut children, Arc::from(Path::new("")), 0, None, &mut nodes);
    TreeSnapshot { nodes, generation }
}

fn emit(
    children: &mut HashMap<Arc<Path>, Vec<Child>>,
    dir: Arc<Path>,
    depth: u16,
    parent: Option<u32>,
    nodes: &mut Vec<TreeNode>,
) -> u32 {
    let Some(list) = children.remove(&dir) else {
        return 0;
    };
    let mut total = 0;
    for child in list {
        let index = nodes.len() as u32;
        let is_folder = child.item.kind == NodeKind::Folder;
        nodes.push(TreeNode {
            name: child.name,
            path: child.item.path.clone(),
            kind: child.item.kind,
            depth,
            parent,
            subtree_end: index + 1,
            note: child.item.note,
            note_count: 0,
            link_out: child.item.link_out,
        });
        if is_folder {
            let count = emit(children, child.item.path, depth + 1, Some(index), nodes);
            let end = nodes.len() as u32;
            let node = &mut nodes[index as usize];
            node.subtree_end = end;
            node.note_count = count;
            total += count;
        } else if child.item.kind == NodeKind::Note {
            total += 1;
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(p: &str, kind: NodeKind) -> Item {
        Item {
            path: Arc::from(Path::new(p)),
            kind,
            note: (kind == NodeKind::Note).then_some(0),
            link_out: None,
        }
    }

    #[test]
    fn order_and_ranges() {
        let items = vec![
            item("b.md", NodeKind::Note),
            item("A", NodeKind::Folder),
            item("A/note 10.md", NodeKind::Note),
            item("A/note 2.md", NodeKind::Note),
            item("A/img.png", NodeKind::Attachment),
            item("A/sub", NodeKind::Folder),
            item("A/sub/x.md", NodeKind::Note),
            item("z", NodeKind::Folder),
        ];
        let t = build(items.into_iter(), 1);
        let names: Vec<_> = t.nodes.iter().map(|n| (n.name.as_ref(), n.depth)).collect();
        assert_eq!(
            names,
            [
                ("A", 0),
                ("sub", 1),
                ("x.md", 2),
                ("img.png", 1),
                ("note 2.md", 1),
                ("note 10.md", 1),
                ("z", 0),
                ("b.md", 0)
            ]
        );
        assert_eq!(t.nodes[0].subtree_end, 6);
        assert_eq!(t.nodes[0].note_count, 3);
        assert_eq!(t.nodes[1].note_count, 1);
        assert_eq!(t.nodes[2].parent, Some(1));
        assert_eq!(t.nodes[6].subtree_end, 7);
    }
}
