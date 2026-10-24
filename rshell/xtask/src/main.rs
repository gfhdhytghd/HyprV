use std::{
    collections::BTreeSet,
    error::Error,
    fs,
    io::BufReader,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}
fn read_png(path: &Path) -> Result<Image> {
    let mut decoder = png::Decoder::new(BufReader::new(fs::File::open(path)?));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut data = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or("PNG exceeds decode limit")?
    ];
    let info = reader.next_frame(&mut data)?;
    if info.bit_depth != png::BitDepth::Eight {
        return Err("expected 8-bit expanded PNG".into());
    }
    let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
    let channels = info.color_type.samples();
    for pixel in data[..info.buffer_size()].chunks_exact(channels) {
        match info.color_type {
            png::ColorType::Rgba => rgba.extend_from_slice(pixel),
            png::ColorType::Rgb => rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]),
            png::ColorType::Grayscale => {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], 255])
            }
            png::ColorType::GrayscaleAlpha => {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]])
            }
            png::ColorType::Indexed => return Err("palette was not expanded".into()),
        }
    }
    Ok(Image {
        width: info.width,
        height: info.height,
        rgba,
    })
}
fn collect(root: &Path, relative: &Path, files: &mut BTreeSet<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root.join(relative))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = relative.join(entry.file_name());
        if file_type.is_dir() {
            collect(root, &path, files)?;
        } else if file_type.is_file()
            && path
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("png"))
        {
            files.insert(path);
        }
    }
    Ok(())
}
fn compare(a: &Image, b: &Image, tolerance: u8) -> Result<(usize, u8)> {
    if (a.width, a.height) != (b.width, b.height) {
        return Err(format!(
            "dimensions differ: {}x{} / {}x{}",
            a.width, a.height, b.width, b.height
        )
        .into());
    }
    let mut changed = 0;
    let mut max = 0;
    for (a, b) in a.rgba.chunks_exact(4).zip(b.rgba.chunks_exact(4)) {
        let delta = a
            .iter()
            .zip(b)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        max = max.max(delta);
        if delta > tolerance {
            changed += 1;
        }
    }
    Ok((changed, max))
}
fn html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn markdown(s: &str) -> String {
    s.replace('|', "\\|").replace(['\n', '\r'], " ")
}
fn run() -> Result<bool> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 || args[0] != "parity" {
        return Err("usage: cargo xtask parity <reference-dir> <actual-dir> [--tolerance 0..255] [--report <directory>]".into());
    }
    let reference = Path::new(&args[1]);
    let actual = Path::new(&args[2]);
    let mut tolerance = 0u8;
    let mut report = PathBuf::from("parity-report");
    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--tolerance" if i + 1 < args.len() => tolerance = args[i + 1].parse()?,
            "--report" if i + 1 < args.len() => report = PathBuf::from(&args[i + 1]),
            _ => return Err(format!("unknown or incomplete option {}", args[i]).into()),
        }
        i += 2;
    }
    let mut left = BTreeSet::new();
    let mut right = BTreeSet::new();
    collect(reference, Path::new(""), &mut left)?;
    collect(actual, Path::new(""), &mut right)?;
    let names: BTreeSet<_> = left.union(&right).cloned().collect();
    if names.is_empty() {
        return Err("no PNG files; parity was not evaluated".into());
    }
    let mut md = format!("# Pixel parity report\n\nPer-channel tolerance: {tolerance}; a pixel fails if any RGBA channel exceeds it.\n\n| File | Result | Pixels over tolerance | Maximum channel delta |\n|---|---|---:|---:|\n");
    let mut page = format!("<!doctype html><meta charset=\"utf-8\"><title>Pixel parity report</title><style>body{{font:16px system-ui;margin:2rem}}table{{border-collapse:collapse}}td,th{{border:1px solid #888;padding:.5rem}}</style><h1>Pixel parity report</h1><p>Per-channel tolerance: {tolerance}</p><table><tr><th>File</th><th>Result</th><th>Pixels over tolerance</th><th>Maximum channel delta</th></tr>");
    let mut passed = true;
    for name in names {
        let result = if !left.contains(&name) {
            Err("missing reference".into())
        } else if !right.contains(&name) {
            Err("missing actual".into())
        } else {
            read_png(&reference.join(&name)).and_then(|a| {
                read_png(&actual.join(&name)).and_then(|b| compare(&a, &b, tolerance))
            })
        };
        let (status, count, delta) = match result {
            Ok((count, delta)) => {
                if count > 0 {
                    passed = false;
                }
                (
                    if count == 0 {
                        "PASS".into()
                    } else {
                        "FAIL".into()
                    },
                    count.to_string(),
                    delta.to_string(),
                )
            }
            Err(e) => {
                passed = false;
                (format!("FAIL: {e}"), "—".into(), "—".into())
            }
        };
        let name = name.to_string_lossy();
        md.push_str(&format!(
            "| {} | {} | {count} | {delta} |\n",
            markdown(&name),
            markdown(&status)
        ));
        page.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{count}</td><td>{delta}</td></tr>",
            html(&name),
            html(&status)
        ));
    }
    page.push_str("</table>");
    fs::create_dir_all(&report)?;
    fs::write(report.join("report.md"), md)?;
    fs::write(report.join("report.html"), page)?;
    println!(
        "{}: {}",
        if passed { "PASS" } else { "FAIL" },
        report.display()
    );
    Ok(passed)
}
fn main() {
    match run() {
        Ok(true) => (),
        Ok(false) => std::process::exit(1),
        Err(e) => {
            eprintln!("xtask: {e}");
            std::process::exit(2);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tolerance_is_per_channel_and_inclusive() {
        let a = Image {
            width: 1,
            height: 1,
            rgba: vec![1, 2, 3, 255],
        };
        let b = Image {
            width: 1,
            height: 1,
            rgba: vec![3, 2, 3, 255],
        };
        assert_eq!(compare(&a, &b, 2).unwrap(), (0, 2));
        assert_eq!(compare(&a, &b, 1).unwrap(), (1, 2));
    }
}
