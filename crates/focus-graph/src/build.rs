use crate::{Edge, EdgeKind, Graph, Node, NodeKind, Spec};
use focus_core::{LinkKind, LinkTarget, NoteId, NoteSummary, Vault};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;

struct NoteLinks {
    summary: NoteSummary,
    links: Vec<(LinkKind, String, LinkTarget)>,
    tags: Vec<String>,
}

fn group_of(path: &Path, groups: &mut Vec<String>, index: &mut HashMap<String, u16>) -> u16 {
    let top = top_folder(path).unwrap_or_default();
    *index.entry(top.clone()).or_insert_with(|| {
        groups.push(top);
        (groups.len() - 1) as u16
    })
}

fn neighbourhood(notes: &[NoteLinks], start: NoteId, depth: u8) -> HashSet<NoteId> {
    let mut adjacency: HashMap<NoteId, Vec<NoteId>> = HashMap::new();
    for n in notes {
        for (_, _, target) in &n.links {
            if let LinkTarget::Note { id, .. } = target {
                adjacency.entry(n.summary.id).or_default().push(*id);
                adjacency.entry(*id).or_default().push(n.summary.id);
            }
        }
    }
    let mut seen = HashSet::from([start]);
    let mut queue = VecDeque::from([(start, 0u8)]);
    while let Some((id, d)) = queue.pop_front() {
        if d >= depth {
            continue;
        }
        for &next in adjacency.get(&id).into_iter().flatten() {
            if seen.insert(next) {
                queue.push_back((next, d + 1));
            }
        }
    }
    seen
}

fn decode(vault: &Vault, summary: NoteSummary, spec: &Spec) -> NoteLinks {
    let links = vault
        .links_of(summary.id)
        .into_iter()
        .map(|(l, t)| (l.kind, l.target, t))
        .collect();
    let tags = if spec.tags {
        vault
            .note(summary.id)
            .map(|m| m.tags.clone())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    NoteLinks {
        summary,
        links,
        tags,
    }
}

fn top_folder(path: &Path) -> Option<String> {
    match path.components().count() {
        0 | 1 => None,
        _ => path
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    }
}

fn visible(spec: &Spec, path: &Path) -> bool {
    spec.hidden_folders.is_empty()
        || top_folder(path).is_none_or(|f| !spec.hidden_folders.contains(&f))
}

struct Assembly<'a> {
    spec: &'a Spec,
    graph: Graph,
    group_index: HashMap<String, u16>,
    node_of: Vec<u32>,
    extra: HashMap<(u8, String), u32>,
    pairs: HashMap<(u32, u32), EdgeKind>,
    tagged: HashSet<(u32, u32)>,
    dead: Vec<bool>,
}

impl<'a> Assembly<'a> {
    fn new(spec: &'a Spec, graph: Graph) -> Assembly<'a> {
        let group_index = graph
            .groups
            .iter()
            .enumerate()
            .map(|(i, g)| (g.clone(), i as u16))
            .collect();
        Assembly {
            spec,
            graph,
            group_index,
            node_of: Vec::new(),
            extra: HashMap::new(),
            pairs: HashMap::new(),
            tagged: HashSet::new(),
            dead: Vec::new(),
        }
    }

    fn node(&self, id: NoteId) -> Option<u32> {
        self.node_of
            .get(id as usize)
            .copied()
            .filter(|&i| i != u32::MAX)
    }

    fn map_note(&mut self, id: NoteId, node: u32) {
        let slot = id as usize;
        if slot >= self.node_of.len() {
            self.node_of.resize(slot + 1, u32::MAX);
        }
        self.node_of[slot] = node;
    }

    fn is_dead(&self, node: u32) -> bool {
        self.dead.get(node as usize).copied().unwrap_or(false)
    }

    fn set_note(&mut self, id: NoteId, path: &Arc<Path>, title: &Arc<str>) -> u32 {
        let group = group_of(path, &mut self.graph.groups, &mut self.group_index);
        let node = Node {
            kind: NodeKind::Note(id),
            label: title.clone(),
            degree: 0,
            group,
            path: Some(path.clone()),
        };
        match self.node(id) {
            Some(i) => {
                self.graph.nodes[i as usize] = node;
                i
            }
            None => {
                self.graph.nodes.push(node);
                let i = (self.graph.nodes.len() - 1) as u32;
                self.map_note(id, i);
                i
            }
        }
    }

    fn extra_node(&mut self, rank: u8, key: String, make: impl FnOnce() -> Node) -> u32 {
        let graph = &mut self.graph;
        *self.extra.entry((rank, key)).or_insert_with(|| {
            graph.nodes.push(make());
            (graph.nodes.len() - 1) as u32
        })
    }

    fn attachment_node(&mut self, p: &Path) -> u32 {
        let p: Arc<Path> = Arc::from(p);
        let label: Arc<str> = Arc::from(
            p.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        self.extra_node(2, p.to_string_lossy().into_owned(), || Node {
            kind: NodeKind::Attachment(p.clone()),
            label,
            degree: 0,
            group: 0,
            path: Some(p.clone()),
        })
    }

    fn add_links(
        &mut self,
        a: u32,
        links: &[(LinkKind, String, LinkTarget)],
        tags: &[String],
        target: &mut dyn FnMut(&mut Self, NoteId) -> Option<u32>,
    ) {
        let spec = self.spec;
        for (kind, raw, link) in links {
            let edge_kind = if matches!(kind, LinkKind::Embed | LinkKind::Include) {
                EdgeKind::Embed
            } else {
                EdgeKind::Link
            };
            let b = match link {
                LinkTarget::Note { id, .. } => match target(self, *id) {
                    Some(b) if b != a && !self.is_dead(b) => b,
                    _ => continue,
                },
                LinkTarget::Attachment(p) if spec.attachments => self.attachment_node(p),
                LinkTarget::Unresolved
                    if spec.unresolved && !raw.is_empty() && *kind != LinkKind::Include =>
                {
                    self.extra_node(1, raw.to_lowercase(), || Node {
                        kind: NodeKind::Unresolved(raw.clone()),
                        label: Arc::from(raw.as_str()),
                        degree: 0,
                        group: 0,
                        path: None,
                    })
                }
                _ => continue,
            };
            self.pairs.entry((a, b)).or_insert(edge_kind);
        }
        for tag in tags {
            let t = self.tag_node(tag);
            self.tagged.insert((a, t));
        }
    }

    fn tag_node(&mut self, name: &str) -> u32 {
        let owned = name.to_string();
        self.extra_node(0, name.to_lowercase(), || Node {
            kind: NodeKind::Tag(owned.clone()),
            label: Arc::from(format!("#{owned}")),
            degree: 0,
            group: 0,
            path: None,
        })
    }

    fn finish(mut self, mut edges: Vec<Edge>) -> Graph {
        if !self.spec.relations.is_empty() {
            let by_path: HashMap<&Path, u32> = self
                .graph
                .nodes
                .iter()
                .enumerate()
                .filter(|(i, n)| matches!(n.kind, NodeKind::Note(_)) && !self.is_dead(*i as u32))
                .filter_map(|(i, n)| Some((n.path.as_deref()?, i as u32)))
                .collect();
            let mut relations: Vec<(u32, u32)> = self
                .spec
                .relations
                .iter()
                .filter_map(|(a, b)| Some((*by_path.get(a.as_path())?, *by_path.get(b.as_path())?)))
                .filter(|(a, b)| a != b)
                .collect();
            relations.sort_unstable();
            relations.dedup();
            edges.extend(relations.into_iter().map(|(a, b)| Edge {
                a,
                b,
                kind: EdgeKind::Relation,
                mutual: false,
            }));
        }
        let mut keys: Vec<(u32, u32)> = self.pairs.keys().copied().collect();
        keys.sort_unstable();
        for &(a, b) in &keys {
            let mutual = self.pairs.contains_key(&(b, a));
            if mutual && b < a {
                continue;
            }
            edges.push(Edge {
                a,
                b,
                kind: self.pairs[&(a, b)],
                mutual,
            });
        }
        let mut tagged: Vec<(u32, u32)> = self.tagged.drain().collect();
        tagged.sort_unstable();
        let mut chained: HashSet<u32> = edges
            .iter()
            .filter(|e| e.kind == EdgeKind::TagParent)
            .map(|e| e.a)
            .collect();
        let mut leaves: Vec<u32> = tagged.iter().map(|&(_, t)| t).collect();
        leaves.sort_unstable();
        leaves.dedup();
        edges.extend(tagged.into_iter().map(|(a, b)| Edge {
            a,
            b,
            kind: EdgeKind::Tagged,
            mutual: false,
        }));
        let mut parents = Vec::new();
        for leaf in leaves {
            let mut child = leaf;
            while chained.insert(child) {
                let NodeKind::Tag(name) = &self.graph.nodes[child as usize].kind else {
                    break;
                };
                let Some(cut) = name.rfind('/') else { break };
                let parent_name = name[..cut].to_string();
                let parent = self.tag_node(&parent_name);
                parents.push(Edge {
                    a: child,
                    b: parent,
                    kind: EdgeKind::TagParent,
                    mutual: false,
                });
                child = parent;
            }
        }
        parents.sort_by_key(|e| (e.a, e.b));
        edges.extend(parents);
        if !chained.is_empty() {
            let parent_of: HashMap<u32, u32> = edges
                .iter()
                .filter(|e| e.kind == EdgeKind::TagParent)
                .map(|e| (e.a, e.b))
                .collect();
            let mut carried = vec![false; self.graph.nodes.len()];
            for e in edges
                .iter()
                .filter(|e| e.kind == EdgeKind::Tagged && !self.is_dead(e.a))
            {
                let mut t = e.b;
                while !carried[t as usize] {
                    carried[t as usize] = true;
                    match parent_of.get(&t) {
                        Some(&p) => t = p,
                        None => break,
                    }
                }
            }
            edges.retain(|e| e.kind != EdgeKind::TagParent || carried[e.a as usize]);
        }
        for n in &mut self.graph.nodes {
            n.degree = 0;
        }
        for e in &edges {
            self.graph.nodes[e.a as usize].degree += 1;
            self.graph.nodes[e.b as usize].degree += 1;
        }
        self.graph.edges = edges;
        let orphans = self.spec.orphans;
        let dead = std::mem::take(&mut self.dead);
        retain_nodes(&mut self.graph, |i, n| {
            !dead.get(i as usize).copied().unwrap_or(false)
                && (n.degree > 0 || (orphans && matches!(n.kind, NodeKind::Note(_))))
        });
        self.graph
    }
}

pub(crate) fn build(vault: &Vault, spec: &Spec) -> Graph {
    let mut summaries: Vec<NoteSummary> = vault
        .summaries()
        .into_iter()
        .filter(|s| visible(spec, &s.path))
        .collect();
    summaries.sort_by(|a, b| a.path.cmp(&b.path));
    let notes: Vec<NoteLinks> = summaries
        .into_par_iter()
        .map(|summary| decode(vault, summary, spec))
        .collect();

    let mut keep: Option<HashSet<NoteId>> =
        spec.only.as_ref().map(|ids| ids.iter().copied().collect());
    if let Some((start, depth)) = spec.around {
        let around = neighbourhood(&notes, start, depth.clamp(1, 4));
        keep = Some(match keep {
            Some(k) => k.intersection(&around).copied().collect(),
            None => around,
        });
    }
    let included: Vec<&NoteLinks> = notes
        .iter()
        .filter(|n| keep.as_ref().is_none_or(|k| k.contains(&n.summary.id)))
        .collect();

    let mut asm = Assembly::new(spec, Graph::default());
    let nodes: Vec<u32> = included
        .iter()
        .map(|n| asm.set_note(n.summary.id, &n.summary.path, &n.summary.title))
        .collect();
    for (n, a) in included.iter().zip(nodes) {
        asm.add_links(a, &n.links, &n.tags, &mut |asm, id| asm.node(id));
    }
    if spec.attachments {
        let mut canvases: Vec<(NoteId, Arc<Path>)> = vault
            .canvases()
            .into_iter()
            .filter(|(_, p)| visible(spec, p))
            .collect();
        canvases.sort_by(|a, b| a.1.cmp(&b.1));
        for (id, path) in canvases {
            let links: Vec<(LinkKind, String, LinkTarget)> = vault
                .links_of(id)
                .into_iter()
                .map(|(l, t)| (l.kind, l.target, t))
                .collect();
            let a = asm.attachment_node(&path);
            asm.add_links(a, &links, &[], &mut |asm, id| asm.node(id));
        }
    }
    asm.finish(Vec::new())
}

#[derive(Debug, Clone, Default)]
pub struct Changes {
    pub changed: Vec<NoteId>,
    pub removed: Vec<NoteId>,
}

pub(crate) fn update(
    vault: &Vault,
    spec: &Spec,
    previous: &Graph,
    changes: &Changes,
) -> Option<Graph> {
    if spec.only.is_some() || spec.around.is_some() {
        return None;
    }
    let n = previous.nodes.len();
    let mut asm = Assembly::new(
        spec,
        Graph {
            nodes: previous.nodes.clone(),
            edges: Vec::new(),
            groups: previous.groups.clone(),
        },
    );
    for (i, node) in previous.nodes.iter().enumerate() {
        let i = i as u32;
        let key = match &node.kind {
            NodeKind::Note(id) => {
                asm.map_note(*id, i);
                continue;
            }
            NodeKind::Tag(t) => (0, t.to_lowercase()),
            NodeKind::Unresolved(t) => (1, t.to_lowercase()),
            NodeKind::Attachment(p) => (2, p.to_string_lossy().into_owned()),
        };
        asm.extra.insert(key, i);
    }

    let mut touched = vec![false; n];
    for &id in changes.changed.iter().chain(&changes.removed) {
        if let Some(i) = asm.node(id) {
            touched[i as usize] = true;
        }
    }
    let mut sources: HashSet<NoteId> = changes.changed.iter().copied().collect();
    for &id in &changes.changed {
        sources.extend(vault.linked_from(id));
    }
    for e in previous
        .edges
        .iter()
        .filter(|e| matches!(e.kind, EdgeKind::Link | EdgeKind::Embed))
    {
        for (from, to) in [(e.a, e.b), (e.b, e.a)] {
            if touched[to as usize]
                && (from == e.a || e.mutual)
                && let NodeKind::Note(id) = previous.nodes[from as usize].kind
            {
                sources.insert(id);
            }
        }
    }
    asm.dead = vec![false; n];
    for &id in changes.changed.iter().chain(&changes.removed) {
        let gone =
            changes.removed.contains(&id) || vault.note(id).is_none_or(|m| !visible(spec, &m.path));
        if gone && let Some(i) = asm.node(id) {
            asm.dead[i as usize] = true;
            sources.remove(&id);
        }
    }
    let mut is_source = vec![false; n];
    for &id in &sources {
        if let Some(i) = asm.node(id) {
            is_source[i as usize] = true;
        }
    }

    let mut kept: Vec<Edge> = Vec::with_capacity(previous.edges.len());
    for e in &previous.edges {
        let (a, b) = (e.a as usize, e.b as usize);
        if asm.dead[a] || asm.dead[b] {
            continue;
        }
        match e.kind {
            EdgeKind::Link | EdgeKind::Embed if is_source[a] || is_source[b] => {
                for (from, to) in [(e.a, e.b), (e.b, e.a)] {
                    if (from == e.a || e.mutual) && !is_source[from as usize] {
                        asm.pairs.entry((from, to)).or_insert(e.kind);
                    }
                }
            }
            EdgeKind::Tagged if is_source[a] => {}
            EdgeKind::Relation => {}
            _ => kept.push(*e),
        }
    }

    let mut decoded: Vec<NoteLinks> = sources
        .into_par_iter()
        .filter_map(|id| {
            let meta = vault.note(id)?;
            let summary = NoteSummary {
                id,
                path: meta.path.clone(),
                title: meta.title.clone(),
                aliases: meta.aliases.clone(),
                size: meta.size,
                modified: meta.modified,
            };
            visible(spec, &summary.path).then(|| decode(vault, summary, spec))
        })
        .collect();
    decoded.sort_by(|a, b| a.summary.path.cmp(&b.summary.path));
    for d in &decoded {
        let a = asm.set_note(d.summary.id, &d.summary.path, &d.summary.title);
        asm.add_links(a, &d.links, &d.tags, &mut |asm, id| {
            if let Some(i) = asm.node(id) {
                return Some(i);
            }
            let meta = vault.note(id).filter(|m| visible(spec, &m.path))?;
            Some(asm.set_note(id, &meta.path, &meta.title))
        });
    }
    for (from, to) in &spec.relations {
        let ends = [from, to].map(|p| {
            vault
                .note_by_path(p)
                .and_then(|id| Some((id, vault.note(id)?)))
                .filter(|(_, m)| visible(spec, &m.path))
        });
        if let [Some(a), Some(b)] = ends {
            for (id, meta) in [a, b] {
                if asm.node(id).is_none() {
                    asm.set_note(id, &meta.path, &meta.title);
                }
            }
        }
    }
    Some(asm.finish(kept))
}

fn retain_nodes(graph: &mut Graph, keep: impl Fn(u32, &Node) -> bool) {
    let mut remap = vec![u32::MAX; graph.nodes.len()];
    let mut nodes = Vec::with_capacity(graph.nodes.len());
    for (i, n) in graph.nodes.drain(..).enumerate() {
        if keep(i as u32, &n) {
            remap[i] = nodes.len() as u32;
            nodes.push(n);
        }
    }
    graph.nodes = nodes;
    graph
        .edges
        .retain(|e| remap[e.a as usize] != u32::MAX && remap[e.b as usize] != u32::MAX);
    for e in &mut graph.edges {
        e.a = remap[e.a as usize];
        e.b = remap[e.b as usize];
    }
}
