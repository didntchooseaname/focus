use focus_md::mermaid::{Theme, init_fonts, rasterize, render_svg};

fn main() -> anyhow::Result<()> {
    let mut extra_fonts = Vec::new();
    let mut args_list = std::env::args().skip(1);
    let mut positional = Vec::new();
    while let Some(arg) = args_list.next() {
        if arg == "--font" {
            let path = args_list
                .next()
                .ok_or_else(|| anyhow::anyhow!("--font needs a file"))?;
            extra_fonts
                .push(std::fs::read(&path).map_err(|e| anyhow::anyhow!("reading {path}: {e}"))?);
        } else {
            positional.push(arg);
        }
    }
    let mut args = positional.into_iter();
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        anyhow::bail!(
            "usage: mermaid_png <input.mmd> <output.png> [dark|light] [scale] [--font <file.ttf>]..."
        );
    };
    let theme = match args.next().as_deref() {
        None | Some("dark") => Theme::dark(),
        Some("light") => Theme::light(),
        Some(other) => anyhow::bail!("unknown theme: {other}"),
    };
    let scale: f32 = args.next().map_or(Ok(2.0), |s| s.parse())?;
    let source = std::fs::read_to_string(&input)?;
    let fonts_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts/");
    let fonts = [
        "Inter-Regular.ttf",
        "Inter-Medium.ttf",
        "Inter-SemiBold.ttf",
        "Inter-Bold.ttf",
    ]
    .iter()
    .map(|name| std::fs::read(format!("{fonts_dir}{name}")))
    .collect::<Result<Vec<_>, _>>()?;
    let fonts: Vec<Vec<u8>> = fonts.into_iter().chain(extra_fonts).collect();
    let font_refs: Vec<&[u8]> = fonts.iter().map(Vec::as_slice).collect();
    init_fonts(&font_refs);
    let started = std::time::Instant::now();
    let svg = render_svg(&source, &theme)?;
    let rendered = started.elapsed();
    std::fs::write(
        std::path::Path::new(&output).with_extension("svg"),
        &svg.svg,
    )?;
    let started = std::time::Instant::now();
    let raster = rasterize(&svg, scale, &[])?;
    let rasterized = started.elapsed();
    let image = image_png(&raster)?;
    std::fs::write(&output, image)?;
    println!(
        "{} : {:.0}x{:.0}, svg {:.1} ms, png {:.1} ms",
        svg.diagram,
        svg.width,
        svg.height,
        rendered.as_secs_f64() * 1e3,
        rasterized.as_secs_f64() * 1e3
    );
    Ok(())
}

fn image_png(raster: &focus_md::mermaid::Raster) -> anyhow::Result<Vec<u8>> {
    use resvg::tiny_skia::{ColorU8, Pixmap};
    let mut pixmap =
        Pixmap::new(raster.width, raster.height).ok_or_else(|| anyhow::anyhow!("empty image"))?;
    for (dst, src) in pixmap
        .pixels_mut()
        .iter_mut()
        .zip(raster.rgba.as_chunks::<4>().0)
    {
        *dst = ColorU8::from_rgba(src[0], src[1], src[2], src[3]).premultiply();
    }
    Ok(pixmap.encode_png()?)
}
