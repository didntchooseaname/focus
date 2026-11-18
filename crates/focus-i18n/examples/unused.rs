use std::path::Path;

use focus_i18n::audit::embedded;

fn collect(dir: &Path, text: &mut String) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|n| n == "target" || n == ".git")
            {
                continue;
            }
            collect(&path, text)?;
        } else if path.extension().is_some_and(|x| x == "rs") {
            text.push_str(&std::fs::read_to_string(&path)?);
            text.push('\n');
        }
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    let folders: Vec<String> = std::env::args().skip(1).collect();
    if folders.is_empty() {
        eprintln!("usage: unused <source folder>...");
        std::process::exit(2);
    }
    let mut text = String::new();
    for folder in &folders {
        collect(Path::new(folder), &mut text)?;
    }
    let catalog = embedded("en");
    let unused: Vec<&String> = catalog
        .messages
        .keys()
        .filter(|key| !key.starts_with('-') && !text.contains(key.as_str()))
        .collect();
    for key in &unused {
        println!("{key}");
    }
    println!(
        "{} of {} keys are not cited",
        unused.len(),
        catalog.messages.len()
    );
    Ok(())
}
