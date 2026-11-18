use std::io::Write;

use flate2::Compression;
use flate2::write::GzEncoder;

pub(crate) fn tar_gz(entries: &[(String, Vec<u8>)]) -> std::io::Result<Vec<u8>> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    for (path, bytes) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        builder.append_data(&mut header, path, bytes.as_slice())?;
    }
    let mut encoder = builder.into_inner()?;
    encoder.flush()?;
    encoder.finish()
}
