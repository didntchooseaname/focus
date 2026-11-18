use crate::{Graph, NodeKind};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

const MAGIC: &[u8; 4] = b"FCSG";
const VERSION: u32 = 1;
const FILE_NAME: &str = "graph-positions.bin";

fn key(graph: &Graph, i: usize) -> Option<String> {
    let n = &graph.nodes[i];
    Some(match &n.kind {
        NodeKind::Note(_) => format!("n:{}", n.path.as_ref()?.to_string_lossy()),
        NodeKind::Attachment(p) => format!("a:{}", p.to_string_lossy()),
        NodeKind::Tag(t) => format!("t:{}", t.to_lowercase()),
        NodeKind::Unresolved(t) => format!("u:{}", t.to_lowercase()),
    })
}

pub(crate) fn save(cache_dir: &Path, graph: &Graph, positions: &[[f32; 2]]) -> anyhow::Result<()> {
    anyhow::ensure!(
        positions.len() == graph.nodes.len(),
        "{} positions for {} nodes",
        positions.len(),
        graph.nodes.len()
    );
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    let entries: Vec<(String, [f32; 2])> = (0..graph.nodes.len())
        .filter_map(|i| key(graph, i).map(|k| (k, positions[i])))
        .collect();
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (k, p) in entries {
        out.extend_from_slice(&(k.len() as u32).to_le_bytes());
        out.extend_from_slice(k.as_bytes());
        out.extend_from_slice(&p[0].to_le_bytes());
        out.extend_from_slice(&p[1].to_le_bytes());
    }
    std::fs::create_dir_all(cache_dir)?;
    let path = cache_dir.join(FILE_NAME);
    let tmp = cache_dir.join(format!(".{FILE_NAME}.{}", std::process::id()));
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(&out)?;
    f.sync_data()?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        anyhow::anyhow!("writing {}: {e}", path.display())
    })
}

fn parse(buf: &[u8]) -> Option<HashMap<String, [f32; 2]>> {
    let mut pos = 0usize;
    let mut take = |n: usize| -> Option<&[u8]> {
        let s = buf.get(pos..pos.checked_add(n)?)?;
        pos += n;
        Some(s)
    };
    if take(4)? != MAGIC || u32::from_le_bytes(take(4)?.try_into().ok()?) != VERSION {
        return None;
    }
    let count = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
    let mut map = HashMap::with_capacity(count.min(buf.len() / 12));
    for _ in 0..count {
        let len = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
        let k = String::from_utf8(take(len)?.to_vec()).ok()?;
        let x = f32::from_le_bytes(take(4)?.try_into().ok()?);
        let y = f32::from_le_bytes(take(4)?.try_into().ok()?);
        if x.is_finite() && y.is_finite() {
            map.insert(k, [x, y]);
        }
    }
    Some(map)
}

pub(crate) fn load(cache_dir: &Path, graph: &Graph) -> Vec<(u32, [f32; 2])> {
    let Some(map) = std::fs::read(cache_dir.join(FILE_NAME))
        .ok()
        .and_then(|b| parse(&b))
    else {
        return Vec::new();
    };
    (0..graph.nodes.len())
        .filter_map(|i| key(graph, i).and_then(|k| map.get(&k).map(|p| (i as u32, *p))))
        .collect()
}
