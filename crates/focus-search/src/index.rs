use crate::IndexProgress;
use crate::fold::fold_for_index;
use crate::prefilter::{Req, trigram_key};
use focus_core::{NoteId, Vault};
use memmap2::Mmap;
use rayon::prelude::*;
use roaring::RoaringBitmap;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAGIC: &[u8; 4] = b"FCSS";
const VERSION: u32 = 2;
pub(crate) const FILE_NAME: &str = "search.bin";
const KEY_RECORD: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DocStamp {
    pub path_hash: u64,
    pub size: u64,
    pub modified: SystemTime,
}

pub(crate) fn path_hash(path: &Path) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in path.as_os_str().as_encoded_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

const SHARDS: usize = 64;

fn shard_of(key: u32) -> usize {
    (key.wrapping_mul(0x9E37_79B1) >> 26) as usize
}

struct Base {
    map: Mmap,
    keys_at: usize,
    nkeys: usize,
    blobs_at: usize,
}

impl Base {
    fn key_at(&self, i: usize) -> (u32, usize, usize) {
        let r = &self.map[self.keys_at + i * KEY_RECORD..self.keys_at + (i + 1) * KEY_RECORD];
        let key = u32::from_le_bytes([r[0], r[1], r[2], r[3]]);
        let len = u32::from_le_bytes([r[4], r[5], r[6], r[7]]) as usize;
        let off =
            u64::from_le_bytes([r[8], r[9], r[10], r[11], r[12], r[13], r[14], r[15]]) as usize;
        (key, off, len)
    }

    fn find(&self, key: u32) -> Option<usize> {
        let (mut lo, mut hi) = (0, self.nkeys);
        while lo < hi {
            let mid = (lo + hi) / 2;
            match self.key_at(mid).0.cmp(&key) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some(mid),
            }
        }
        None
    }

    fn list(&self, i: usize) -> Option<RoaringBitmap> {
        let (_, off, len) = self.key_at(i);
        RoaringBitmap::deserialize_from(&self.map[self.blobs_at + off..self.blobs_at + off + len])
            .ok()
    }
}

pub(crate) struct Postings {
    base: Option<Base>,
    stale: RoaringBitmap,
    added: Vec<HashMap<u32, RoaringBitmap>>,
    pub docs: HashMap<NoteId, DocStamp>,
    corrupt: AtomicBool,
}

impl Default for Postings {
    fn default() -> Self {
        Postings {
            base: None,
            stale: RoaringBitmap::new(),
            added: (0..SHARDS).map(|_| HashMap::new()).collect(),
            docs: HashMap::new(),
            corrupt: AtomicBool::new(false),
        }
    }
}

pub(crate) fn doc_trigrams(text: &str, buf: &mut Vec<u8>, out: &mut Vec<u32>) {
    fold_for_index(text, buf);
    out.clear();
    out.extend(buf.windows(3).map(|w| {
        let k = trigram_key(w);
        (shard_of(k) as u32) << 24 | k
    }));
    out.sort_unstable();
    out.dedup();
}

fn add_id(b: &mut RoaringBitmap, id: NoteId) {
    if !b.push(id) {
        b.insert(id);
    }
}

impl Postings {
    pub fn is_corrupt(&self) -> bool {
        self.corrupt.load(Ordering::Relaxed)
    }

    pub fn get(&self, key: u32) -> Option<RoaringBitmap> {
        let mut out: Option<RoaringBitmap> = None;
        if let Some(base) = &self.base
            && let Some(i) = base.find(key)
        {
            match base.list(i) {
                Some(mut b) => {
                    b -= &self.stale;
                    out = Some(b);
                }
                None => self.corrupt.store(true, Ordering::Relaxed),
            }
        }
        if let Some(a) = self.added[shard_of(key)].get(&key) {
            out = Some(match out {
                Some(b) => b | a,
                None => a.clone(),
            });
        }
        out.filter(|b| !b.is_empty())
    }

    pub fn eval(&self, req: &Req) -> Option<RoaringBitmap> {
        let r = self.eval_inner(req);
        if self.is_corrupt() { None } else { r }
    }

    fn eval_inner(&self, req: &Req) -> Option<RoaringBitmap> {
        match req {
            Req::All => None,
            Req::Tri(k) => Some(self.get(*k).unwrap_or_default()),
            Req::And(items) => {
                let mut tris: Vec<RoaringBitmap> = Vec::new();
                let mut others = Vec::new();
                for i in items {
                    match i {
                        Req::Tri(k) => match self.get(*k) {
                            Some(b) => tris.push(b),
                            None => return Some(RoaringBitmap::new()),
                        },
                        other => others.push(other),
                    }
                }
                tris.sort_by_key(|b| b.len());
                let mut acc: Option<RoaringBitmap> = None;
                for b in tris {
                    acc = Some(match acc {
                        None => b,
                        Some(a) => a & b,
                    });
                    if acc.as_ref().is_some_and(|a| a.is_empty()) {
                        return acc;
                    }
                }
                for o in others {
                    if let Some(b) = self.eval_inner(o) {
                        acc = Some(match acc {
                            None => b,
                            Some(a) => a & b,
                        });
                    }
                }
                acc
            }
            Req::Or(items) => {
                let mut acc = RoaringBitmap::new();
                for i in items {
                    acc |= self.eval_inner(i)?;
                }
                Some(acc)
            }
        }
    }

    pub fn trigram_count(&self) -> usize {
        let base = self.base.as_ref().map_or(0, |b| b.nkeys);
        let extra = self
            .added
            .iter()
            .flatten()
            .filter(|(k, _)| self.base.as_ref().is_none_or(|b| b.find(**k).is_none()))
            .count();
        base + extra
    }

    pub fn memory_bytes(&self) -> u64 {
        let added: u64 = self
            .added
            .iter()
            .flatten()
            .map(|(_, b)| b.serialized_size() as u64 + 48)
            .sum();
        added + self.stale.serialized_size() as u64 + self.docs.len() as u64 * 40
    }

    pub fn remove(&mut self, ids: &RoaringBitmap) {
        if ids.is_empty() {
            return;
        }
        self.stale |= ids;
        self.added.par_iter_mut().for_each(|shard| {
            for b in shard.values_mut() {
                *b -= ids;
            }
            shard.retain(|_, b| !b.is_empty());
        });
        for id in ids {
            self.docs.remove(&id);
        }
    }

    pub fn add(
        &mut self,
        vault: &Vault,
        docs: Vec<(NoteId, DocStamp)>,
        progress: &(dyn Fn(IndexProgress) + Sync),
    ) {
        let total = docs.len();
        if total == 0 {
            return;
        }
        let mut docs = docs;
        docs.sort_unstable_by_key(|(id, _)| *id);
        const BATCH: usize = 8192;
        let mut done = 0;
        for batch in docs.chunks(BATCH) {
            let lists: Vec<Option<(Vec<u32>, [u32; SHARDS + 1])>> = batch
                .par_iter()
                .map_init(
                    || (Vec::new(), Vec::new()),
                    |(buf, tris), (id, _)| {
                        let text = vault.read(*id).ok()?;
                        doc_trigrams(&text, buf, tris);
                        let mut bounds = [0u32; SHARDS + 1];
                        for (s, b) in bounds.iter_mut().enumerate() {
                            *b = tris.partition_point(|&v| (v >> 24) < s as u32) as u32;
                        }
                        Some((tris.clone(), bounds))
                    },
                )
                .collect();
            self.added
                .par_iter_mut()
                .enumerate()
                .for_each(|(s, shard)| {
                    for ((id, _), list) in batch.iter().zip(&lists) {
                        let Some((tris, bounds)) = list else { continue };
                        for &v in &tris[bounds[s] as usize..bounds[s + 1] as usize] {
                            add_id(shard.entry(v & 0x00FF_FFFF).or_default(), *id);
                        }
                    }
                });
            for ((id, stamp), list) in batch.iter().zip(&lists) {
                if list.is_some() {
                    self.docs.insert(*id, *stamp);
                }
            }
            done += batch.len();
            progress(IndexProgress { done, total });
        }
    }

    #[cfg(test)]
    pub fn materialize(&self) -> std::collections::BTreeMap<u32, RoaringBitmap> {
        let mut keys: Vec<u32> = self.added.iter().flat_map(|s| s.keys().copied()).collect();
        if let Some(base) = &self.base {
            keys.extend((0..base.nkeys).map(|i| base.key_at(i).0));
        }
        keys.sort_unstable();
        keys.dedup();
        keys.into_iter()
            .filter_map(|k| self.get(k).map(|b| (k, b)))
            .collect()
    }

    pub fn write_and_remap(&mut self, root: &Path, path: &Path) -> std::io::Result<()> {
        let mut keys: Vec<u32> = self.added.iter().flat_map(|s| s.keys().copied()).collect();
        if let Some(base) = &self.base {
            keys.extend((0..base.nkeys).map(|i| base.key_at(i).0));
        }
        keys.sort_unstable();
        keys.dedup();

        let dir = path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)?;
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        let result = (|| -> std::io::Result<()> {
            let mut f = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
            let mut head = Vec::new();
            head.extend_from_slice(MAGIC);
            head.extend_from_slice(&VERSION.to_le_bytes());
            let root = root.to_string_lossy();
            head.extend_from_slice(&(root.len() as u32).to_le_bytes());
            head.extend_from_slice(root.as_bytes());
            let mut docs: Vec<_> = self.docs.iter().collect();
            docs.sort_by_key(|(id, _)| **id);
            head.extend_from_slice(&(docs.len() as u32).to_le_bytes());
            for (id, d) in docs {
                head.extend_from_slice(&id.to_le_bytes());
                head.extend_from_slice(&d.path_hash.to_le_bytes());
                head.extend_from_slice(&d.size.to_le_bytes());
                let t = d.modified.duration_since(UNIX_EPOCH).unwrap_or_default();
                head.extend_from_slice(&t.as_secs().to_le_bytes());
                head.extend_from_slice(&t.subsec_nanos().to_le_bytes());
            }
            f.write_all(&head)?;
            let blobs_at = head.len() as u64;
            let mut table = Vec::with_capacity(keys.len() * KEY_RECORD);
            let mut nkeys = 0u32;
            let mut offset = 0u64;
            for batch in keys.chunks(4096) {
                let blobs: Vec<Option<Vec<u8>>> = batch
                    .par_iter()
                    .map(|&k| {
                        self.get(k).map(|b| {
                            let mut v = Vec::with_capacity(b.serialized_size());
                            b.serialize_into(&mut v).expect("writing into a Vec");
                            v
                        })
                    })
                    .collect();
                for (k, blob) in batch.iter().zip(blobs) {
                    let Some(blob) = blob else { continue };
                    table.extend_from_slice(&k.to_le_bytes());
                    table.extend_from_slice(&(blob.len() as u32).to_le_bytes());
                    table.extend_from_slice(&offset.to_le_bytes());
                    offset += blob.len() as u64;
                    nkeys += 1;
                    f.write_all(&blob)?;
                }
            }
            f.write_all(&table)?;
            f.write_all(&nkeys.to_le_bytes())?;
            f.write_all(&(blobs_at + offset).to_le_bytes())?;
            let file = f.into_inner().map_err(|e| e.into_error())?;
            file.sync_data()?;
            Ok(())
        })();
        if let Err(e) = result {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        if self.is_corrupt() {
            let _ = std::fs::remove_file(&tmp);
            return Err(std::io::Error::other("an index list does not decode"));
        }
        #[cfg(windows)]
        let had_base = self.base.take().is_some();
        if let Err(error) = std::fs::rename(&tmp, path) {
            #[cfg(windows)]
            if had_base {
                self.base = std::fs::File::open(path)
                    .ok()
                    .and_then(|file| Postings::open_file(root, &file))
                    .and_then(|old| old.base);
                if self.base.is_none() {
                    self.corrupt
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            let _ = std::fs::remove_file(&tmp);
            return Err(error);
        }
        let file = std::fs::File::open(path)?;
        let fresh = Postings::open_file(root, &file)
            .ok_or_else(|| std::io::Error::other("rewritten index is unreadable"))?;
        self.base = fresh.base;
        self.stale = RoaringBitmap::new();
        self.added = (0..SHARDS).map(|_| HashMap::new()).collect();
        Ok(())
    }

    pub fn open_file(root: &Path, file: &std::fs::File) -> Option<Postings> {
        // SAFETY: the file is never modified in place; `write_and_remap` writes a new
        // file and renames it, which leaves the current mapping intact.
        let map = unsafe { Mmap::map(file) }.ok()?;
        let mut r = Reader { buf: &map, pos: 0 };
        if r.take(4)? != MAGIC || r.u32()? != VERSION || r.str()? != root.to_string_lossy() {
            return None;
        }
        let ndocs = r.count(32)?;
        let mut docs = HashMap::with_capacity(ndocs);
        for _ in 0..ndocs {
            let id = r.u32()?;
            let path_hash = r.u64()?;
            let size = r.u64()?;
            let secs = r.u64()?;
            let nanos = r.u32()?;
            let modified = UNIX_EPOCH.checked_add(Duration::new(secs, nanos))?;
            docs.insert(
                id,
                DocStamp {
                    path_hash,
                    size,
                    modified,
                },
            );
        }
        let blobs_at = r.pos;
        let len = map.len();
        let trailer = map.get(len.checked_sub(12)?..)?;
        let nkeys = u32::from_le_bytes(trailer[..4].try_into().ok()?) as usize;
        let keys_at = u64::from_le_bytes(trailer[4..].try_into().ok()?) as usize;
        if keys_at < blobs_at || keys_at.checked_add(nkeys.checked_mul(KEY_RECORD)?)? != len - 12 {
            return None;
        }
        let base = Base {
            map,
            keys_at,
            nkeys,
            blobs_at,
        };
        let mut expected = 0usize;
        let mut prev: Option<u32> = None;
        for i in 0..nkeys {
            let (k, off, len) = base.key_at(i);
            if prev.is_some_and(|p| p >= k) || k > 0x00FF_FFFF || off != expected {
                return None;
            }
            expected = off.checked_add(len)?;
            prev = Some(k);
        }
        if base.blobs_at.checked_add(expected)? != base.keys_at {
            return None;
        }
        Some(Postings {
            base: Some(base),
            docs,
            ..Default::default()
        })
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let s = self.buf.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn str(&mut self) -> Option<String> {
        let n = self.u32()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
    fn count(&mut self, min_size: usize) -> Option<usize> {
        let n = self.u32()? as usize;
        (n.saturating_mul(min_size) <= self.buf.len() - self.pos).then_some(n)
    }
}
