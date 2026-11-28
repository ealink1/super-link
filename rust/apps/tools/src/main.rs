//! Reproducible asset tools, replacing the retired build-time Go commands.
mod sign;

use std::{
    io::{Cursor, Write},
    path::Path,
};
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("app-icon") => icon(Path::new(
            args.get(1).map(String::as_str).unwrap_or("assets/branding"),
        )),
        Some("catalog-zip") => catalog(Path::new(
            args.get(1)
                .map(String::as_str)
                .unwrap_or("internal/upstream/i18n"),
        )),
        Some("sign") => sign::run(&args[1..]),
        _ => Err(
            "usage: superlink-tools app-icon [directory] | catalog-zip [directory] | \
             sign --version <v> [--assets-dir <dir>] [--channel stable|preview] \
             [--key-file <path>] | sign --generate-key-file <path>"
                .into(),
        ),
    }
}
fn icon(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let source = image::open(directory.join("superlink-rounded.png"))?;
    let mut chunks = Vec::new();
    for (edge, kind) in [
        (16, b"icp4"),
        (32, b"icp5"),
        (64, b"icp6"),
        (128, b"ic07"),
        (256, b"ic08"),
        (512, b"ic09"),
        (1024, b"ic10"),
    ] {
        let resized = source.resize_exact(edge, edge, image::imageops::FilterType::CatmullRom);
        let mut encoded = Cursor::new(Vec::new());
        resized.write_to(&mut encoded, image::ImageFormat::Png)?;
        let bytes = encoded.into_inner();
        if edge == 256 {
            std::fs::write(directory.join("superlink.png"), &bytes)?;
        }
        chunks.extend(kind);
        chunks.extend(((bytes.len() + 8) as u32).to_be_bytes());
        chunks.extend(bytes);
    }
    let mut result = b"icns".to_vec();
    result.extend(((chunks.len() + 8) as u32).to_be_bytes());
    result.extend(chunks);
    std::fs::write(directory.join("superlink.icns"), result)?;
    Ok(())
}
fn catalog(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(9))
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644);
    for language in ["zh-CN", "zh-TW", "en-US", "ja-JP", "de-DE", "ru-RU"] {
        let name = format!("{language}.json");
        let source = std::fs::read_to_string(directory.join(&name))?.replace("\r\n", "\n");
        let _: serde_json::Value = serde_json::from_str(&source)?;
        archive.start_file(name, options)?;
        archive.write_all(source.as_bytes())?;
    }
    std::fs::write(
        directory.join("catalog.zip"),
        archive.finish()?.into_inner(),
    )?;
    Ok(())
}
