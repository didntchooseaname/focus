use std::collections::{HashMap, HashSet, VecDeque};

use focus_core::{LinkTarget, NoteId, Vault};

#[derive(Default)]
pub struct Graph {
    pub out: HashMap<NoteId, Vec<NoteId>>,
    pub inn: HashMap<NoteId, Vec<NoteId>>,
}

impl Graph {
    pub fn build(vault: &Vault) -> Graph {
        let mut graph = Graph::default();
        let mut seen: HashSet<(NoteId, NoteId)> = HashSet::new();
        for meta in vault.notes() {
            for link in &meta.links {
                if let LinkTarget::Note { id, .. } = vault.resolve_link(meta.id, &link.target)
                    && id != meta.id
                    && seen.insert((meta.id, id))
                {
                    graph.out.entry(meta.id).or_default().push(id);
                    graph.inn.entry(id).or_default().push(meta.id);
                }
            }
        }
        graph
    }

    pub fn outgoing(&self, id: NoteId) -> &[NoteId] {
        self.out.get(&id).map_or(&[], Vec::as_slice)
    }

    pub fn incoming(&self, id: NoteId) -> &[NoteId] {
        self.inn.get(&id).map_or(&[], Vec::as_slice)
    }

    pub fn around(&self, id: NoteId) -> Vec<NoteId> {
        let mut all: Vec<NoteId> = self
            .outgoing(id)
            .iter()
            .chain(self.incoming(id))
            .copied()
            .collect();
        all.sort_unstable();
        all.dedup();
        all
    }

    pub fn layers(&self, start: NoteId, depth: usize) -> Vec<Vec<NoteId>> {
        let mut seen: HashSet<NoteId> = HashSet::from([start]);
        let mut frontier = vec![start];
        let mut layers = Vec::new();
        for _ in 0..depth {
            let mut next = Vec::new();
            for id in &frontier {
                for neighbor in self.around(*id) {
                    if seen.insert(neighbor) {
                        next.push(neighbor);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            layers.push(next.clone());
            frontier = next;
        }
        layers
    }

    pub fn shortest_path(&self, from: NoteId, to: NoteId) -> Option<Vec<NoteId>> {
        if from == to {
            return Some(vec![from]);
        }
        let mut parent: HashMap<NoteId, NoteId> = HashMap::new();
        let mut queue = VecDeque::from([from]);
        let mut seen: HashSet<NoteId> = HashSet::from([from]);
        while let Some(current) = queue.pop_front() {
            for next in self.around(current) {
                if !seen.insert(next) {
                    continue;
                }
                parent.insert(next, current);
                if next == to {
                    let mut path = vec![to];
                    let mut at = to;
                    while let Some(p) = parent.get(&at) {
                        path.push(*p);
                        at = *p;
                    }
                    path.reverse();
                    return Some(path);
                }
                queue.push_back(next);
            }
        }
        None
    }
}
