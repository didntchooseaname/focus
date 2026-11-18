use focus_md::highlight::{detect, highlight};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (Some(lang), Some(path)) = (args.next(), args.next()) else {
        anyhow::bail!("usage: highlight_dump <language|auto> <file>");
    };
    let code = std::fs::read_to_string(path)?;
    let lang = if lang == "auto" {
        let found = detect(&code);
        println!("detected: {found:?}");
        found.map_or("text", |d| d.lang).to_string()
    } else {
        lang
    };
    for span in highlight(&code, &lang) {
        println!("{:?} {:?}", span.token, &code[span.range]);
    }
    Ok(())
}
