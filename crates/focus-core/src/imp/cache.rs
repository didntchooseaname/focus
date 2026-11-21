use super::config::fnv1a;
use super::parse::{Parsed, TaskLine};
use crate::{Heading, LinkKind, NoteId, OutLink, ParsedMeta, PropValue};
use rayon::prelude::*;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAGIC: &[u8; 4] = b"FCSC";
const VERSION: u32 = 10;
pub(crate) const FILE_NAME: &str = "meta.bin";

#[derive(Debug, Clone)]
pub(crate) struct CachedNote {
    pub path: Arc<Path>,
    pub id: NoteId,
    pub size: u64,
    pub modified: SystemTime,
    pub title: Arc<str>,
    pub title_from_name: bool,
    pub aliases: Box<[String]>,
    pub blob: Arc<[u8]>,
}

pub(crate) struct CacheContents {
    pub next_id: NoteId,
    pub notes: Vec<CachedNote>,
}

#[derive(Default)]
struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.buf.extend_from_slice(s.as_bytes());
    }
    fn opt_str(&mut self, s: Option<&str>) {
        match s {
            Some(s) => {
                self.u8(1);
                self.str(s);
            }
            None => self.u8(0),
        }
    }
    fn time(&mut self, t: SystemTime) {
        let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
        self.u64(d.as_secs());
        self.u32(d.subsec_nanos());
    }
    fn prop(&mut self, v: &PropValue) {
        match v {
            PropValue::Null => self.u8(0),
            PropValue::Bool(b) => {
                self.u8(1);
                self.u8(*b as u8);
            }
            PropValue::Number(n) => {
                self.u8(2);
                self.u64(n.to_bits());
            }
            PropValue::Text(s) => {
                self.u8(3);
                self.str(s);
            }
            PropValue::List(items) => {
                self.u8(4);
                self.u32(items.len() as u32);
                for i in items {
                    self.prop(i);
                }
            }
            PropValue::Map(entries) => {
                self.u8(5);
                self.props(entries);
            }
        }
    }
    fn props(&mut self, entries: &[(String, PropValue)]) {
        self.u32(entries.len() as u32);
        for (k, v) in entries {
            self.str(k);
            self.prop(v);
        }
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

type R<T> = Option<T>;

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> R<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let s = self.buf.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }
    fn u8(&mut self) -> R<u8> {
        Some(self.bytes(1)?[0])
    }
    fn u32(&mut self) -> R<u32> {
        Some(u32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> R<u64> {
        Some(u64::from_le_bytes(self.bytes(8)?.try_into().ok()?))
    }
    fn str(&mut self) -> R<String> {
        let n = self.u32()? as usize;
        String::from_utf8(self.bytes(n)?.to_vec()).ok()
    }
    fn opt_str(&mut self) -> R<Option<String>> {
        match self.u8()? {
            0 => Some(None),
            1 => Some(Some(self.str()?)),
            _ => None,
        }
    }
    fn time(&mut self) -> R<SystemTime> {
        let secs = self.u64()?;
        let nanos = self.u32()?;
        UNIX_EPOCH.checked_add(Duration::new(secs, nanos))
    }
    fn len(&mut self) -> R<usize> {
        let n = self.u32()? as usize;
        (n <= self.buf.len() - self.pos).then_some(n)
    }
    fn prop(&mut self, depth: u32) -> R<PropValue> {
        if depth > 64 {
            return None;
        }
        Some(match self.u8()? {
            0 => PropValue::Null,
            1 => PropValue::Bool(self.u8()? != 0),
            2 => PropValue::Number(f64::from_bits(self.u64()?)),
            3 => PropValue::Text(self.str()?),
            4 => {
                let n = self.len()?;
                let mut items = Vec::with_capacity(n);
                for _ in 0..n {
                    items.push(self.prop(depth + 1)?);
                }
                PropValue::List(items)
            }
            5 => PropValue::Map(self.props(depth + 1)?),
            _ => return None,
        })
    }
    fn props(&mut self, depth: u32) -> R<Vec<(String, PropValue)>> {
        let n = self.len()?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let k = self.str()?;
            out.push((k, self.prop(depth)?));
        }
        Some(out)
    }
    fn strs(&mut self) -> R<Vec<String>> {
        let n = self.len()?;
        (0..n).map(|_| self.str()).collect()
    }
}

fn put_link(w: &mut Writer, l: &OutLink) {
    w.u8(match l.kind {
        LinkKind::Wiki => 0,
        LinkKind::Markdown => 1,
        LinkKind::Embed => 2,
        LinkKind::Property => 3,
        LinkKind::Diagram => 4,
        LinkKind::Include => 5,
    });
    w.str(&l.target);
    w.opt_str(l.anchor.as_deref());
    w.u32(l.line);
}

fn get_links(r: &mut Reader) -> R<Vec<OutLink>> {
    let n = r.len()?;
    let mut links = Vec::with_capacity(n);
    for _ in 0..n {
        let kind = match r.u8()? {
            0 => LinkKind::Wiki,
            1 => LinkKind::Markdown,
            2 => LinkKind::Embed,
            3 => LinkKind::Property,
            4 => LinkKind::Diagram,
            5 => LinkKind::Include,
            _ => return None,
        };
        let target = r.str()?;
        let anchor = r.opt_str()?;
        links.push(OutLink {
            kind,
            target,
            anchor,
            line: r.u32()?,
        });
    }
    Some(links)
}

pub(crate) fn encode_parsed(p: &Parsed) -> Vec<u8> {
    let mut w = Writer::default();
    let m = &p.meta;
    w.u32(m.links.len() as u32);
    for l in &m.links {
        put_link(&mut w, l);
    }
    w.opt_str(m.title.as_deref());
    w.u32(m.aliases.len() as u32);
    for a in &m.aliases {
        w.str(a);
    }
    w.u32(m.tags.len() as u32);
    for t in &m.tags {
        w.str(t);
    }
    w.u32(m.headings.len() as u32);
    for h in &m.headings {
        w.u8(h.level);
        w.str(&h.text);
        w.u32(h.line);
    }
    w.u32(m.blocks.len() as u32);
    for (id, line) in &m.blocks {
        w.str(id);
        w.u32(*line);
    }
    w.props(&m.props);
    w.u32(m.words);
    w.u32(m.tasks_open);
    w.u32(m.tasks_done);
    w.u32(m.body_start_line);
    w.u32(p.tasks.len() as u32);
    for t in &p.tasks {
        w.u32(t.line);
        w.str(&t.text);
        w.u8(t.done as u8);
    }
    w.buf
}

pub(crate) fn decode_links(blob: &[u8]) -> R<Vec<OutLink>> {
    get_links(&mut Reader { buf: blob, pos: 0 })
}

pub(crate) fn decode_parsed(blob: &[u8]) -> R<Parsed> {
    let mut r = Reader { buf: blob, pos: 0 };
    let links = get_links(&mut r)?;
    let title = r.opt_str()?;
    let aliases = r.strs()?;
    let tags = r.strs()?;
    let n = r.len()?;
    let mut headings = Vec::with_capacity(n);
    for _ in 0..n {
        let level = r.u8()?;
        let text = r.str()?;
        headings.push(Heading {
            level,
            text,
            line: r.u32()?,
        });
    }
    let n = r.len()?;
    let mut blocks = Vec::with_capacity(n);
    for _ in 0..n {
        let id = r.str()?;
        blocks.push((id, r.u32()?));
    }
    let props = r.props(0)?;
    let words = r.u32()?;
    let tasks_open = r.u32()?;
    let tasks_done = r.u32()?;
    let body_start_line = r.u32()?;
    let n = r.len()?;
    let mut tasks = Vec::with_capacity(n);
    for _ in 0..n {
        let line = r.u32()?;
        let text = r.str()?;
        tasks.push(TaskLine {
            line,
            text,
            done: r.u8()? != 0,
        });
    }
    (r.pos == blob.len()).then_some(())?;
    Some(Parsed {
        meta: ParsedMeta {
            title,
            aliases,
            tags,
            headings,
            blocks,
            props,
            links,
            words,
            tasks_open,
            tasks_done,
            body_start_line,
        },
        tasks,
    })
}

fn encode_note(n: &CachedNote) -> Vec<u8> {
    let mut w = Writer::default();
    w.str(&n.path.to_string_lossy());
    w.u32(n.id);
    w.u64(n.size);
    w.time(n.modified);
    w.str(&n.title);
    w.u8(n.title_from_name as u8);
    w.u32(n.aliases.len() as u32);
    for a in n.aliases.iter() {
        w.str(a);
    }
    w.u64(fnv1a(&n.blob));
    w.u32(n.blob.len() as u32);
    w.buf.extend_from_slice(&n.blob);
    w.buf
}

fn decode_note(buf: &[u8]) -> R<CachedNote> {
    let mut r = Reader { buf, pos: 0 };
    let path: Arc<Path> = Arc::from(Path::new(&r.str()?));
    let id = r.u32()?;
    let size = r.u64()?;
    let modified = r.time()?;
    let title: Arc<str> = Arc::from(r.str()?);
    let title_from_name = r.u8()? != 0;
    let aliases = r.strs()?.into_boxed_slice();
    let sum = r.u64()?;
    let n = r.len()?;
    let blob = r.bytes(n)?;
    (r.pos == buf.len() && fnv1a(blob) == sum).then_some(())?;
    Some(CachedNote {
        path,
        id,
        size,
        modified,
        title,
        title_from_name,
        aliases,
        blob: Arc::from(blob),
    })
}

pub(crate) fn encode_records(root: &Path, next_id: NoteId, notes: &[CachedNote]) -> Vec<u8> {
    let records: Vec<Vec<u8>> = notes.par_iter().map(encode_note).collect();
    let mut w = Writer::default();
    w.buf.extend_from_slice(MAGIC);
    w.u32(VERSION);
    w.str(&root.to_string_lossy());
    w.u32(next_id);
    w.u32(records.len() as u32);
    let mut offset = 0u64;
    for r in &records {
        offset += r.len() as u64;
        w.u64(offset);
    }
    let total: usize = records.iter().map(Vec::len).sum();
    w.buf.reserve(total);
    for r in records {
        w.buf.extend_from_slice(&r);
    }
    w.buf
}

pub(crate) fn decode(root: &Path, buf: &[u8]) -> Option<CacheContents> {
    let mut r = Reader { buf, pos: 0 };
    if r.bytes(4)? != MAGIC || r.u32()? != VERSION || r.str()? != root.to_string_lossy() {
        return None;
    }
    let next_id = r.u32()?;
    let count = r.len()?;
    let mut ends = Vec::with_capacity(count);
    for _ in 0..count {
        ends.push(r.u64()? as usize);
    }
    let body = &buf[r.pos..];
    let mut start = 0;
    let mut ranges = Vec::with_capacity(count);
    for &end in &ends {
        if end < start || end > body.len() {
            return None;
        }
        ranges.push(start..end);
        start = end;
    }
    if start != body.len() {
        return None;
    }
    let notes: Option<Vec<CachedNote>> = ranges
        .into_par_iter()
        .map(|range| decode_note(&body[range]))
        .collect();
    let notes = notes?;
    if notes.iter().any(|n| n.id >= next_id) {
        return None;
    }
    Some(CacheContents { next_id, notes })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<CachedNote> {
        let parsed = super::super::parse::parse(
            "---\ntags: [a]\nn: {x: 1}\n---\n# T\n- [ ] t\n[[L#a]] ![](i.png)\n",
        );
        vec![CachedNote {
            path: Arc::from(Path::new("dir/n.md")),
            id: 4,
            size: 10,
            modified: UNIX_EPOCH + Duration::new(100, 7),
            title: Arc::from("T"),
            title_from_name: false,
            aliases: Box::new([]),
            blob: Arc::from(encode_parsed(&parsed)),
        }]
    }

    #[test]
    fn round_trip() {
        let root = Path::new("/v");
        let c = sample();
        let bytes = encode_records(root, 5, &c);
        let d = decode(root, &bytes).unwrap();
        assert_eq!(d.next_id, 5);
        let (a, b) = (&c[0], &d.notes[0]);
        assert_eq!(
            (a.id, a.size, a.modified, &a.path, &a.title),
            (b.id, b.size, b.modified, &b.path, &b.title)
        );
        assert_eq!(a.blob, b.blob);
        let p = decode_parsed(&b.blob).unwrap();
        let original = super::super::parse::parse(
            "---\ntags: [a]\nn: {x: 1}\n---\n# T\n- [ ] t\n[[L#a]] ![](i.png)\n",
        );
        assert_eq!(p.meta.links, original.meta.links);
        assert_eq!(p.meta.props, original.meta.props);
        assert_eq!(p.meta.headings, original.meta.headings);
        assert_eq!(p.tasks, original.tasks);
        assert_eq!(decode_links(&b.blob).unwrap(), original.meta.links);
    }

    #[test]
    fn corrupt_or_foreign_is_rejected() {
        let root = Path::new("/v");
        let bytes = encode_records(root, 5, &sample());
        assert!(decode(Path::new("/autre"), &bytes).is_none());
        for cut in [0, 3, 10, bytes.len() / 2, bytes.len() - 1] {
            assert!(decode(root, &bytes[..cut]).is_none());
        }
        let mut flipped = bytes.clone();
        let last = flipped.len() - 3;
        flipped[last] ^= 0xff;
        let _ = decode(root, &flipped);
        let mut other_version = bytes;
        other_version[4] ^= 1;
        assert!(decode(root, &other_version).is_none());
    }
}
