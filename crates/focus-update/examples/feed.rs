use std::path::PathBuf;

use sha2::{Digest as _, Sha256};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1).map(PathBuf::from);
    let (Some(folder), Some(version)) = (arguments.next(), arguments.next()) else {
        return Err("usage: feed <folder> <version> <release file>...".into());
    };
    let version = version.to_string_lossy().into_owned();
    let latest = folder.join("latest").join("download");
    let tagged = folder.join("download").join(format!("v{version}"));
    std::fs::create_dir_all(&latest)?;
    std::fs::create_dir_all(&tagged)?;
    let mut list = String::new();
    for file in arguments {
        let name = file.file_name().ok_or("a release file has no name")?;
        let bytes = std::fs::read(&file)?;
        let digest: String = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        list.push_str(&format!("{digest}  {}\n", name.to_string_lossy()));
        std::fs::write(tagged.join(name), bytes)?;
    }
    let pair = minisign::KeyPair::generate_unencrypted_keypair()?;
    let signature = minisign::sign(
        None,
        &pair.sk,
        std::io::Cursor::new(list.as_bytes()),
        Some(&format!("Focus {version}")),
        None,
    )?;
    std::fs::write(latest.join("SHA256SUMS.txt"), &list)?;
    std::fs::write(
        latest.join("SHA256SUMS.txt.minisig"),
        signature.into_string(),
    )?;
    std::fs::write(folder.join("release.pub"), pair.pk.to_box()?.into_string())?;
    Ok(())
}
