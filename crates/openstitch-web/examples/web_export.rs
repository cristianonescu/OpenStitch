//! The web editor's File ▸ Open and File ▸ Export, run natively: the reference the browser
//! build is compared with (web/smoke.mjs, CI job `web`).
//!
//! `cargo run -p openstitch-web --example web_export -- INPUT OUT_DIR FORMAT...` writes
//! `OUT_DIR/<input file name>.<format>` for each format, the bytes the web editor downloads.

use openstitch_web::WebSession;
use std::path::Path;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, out_dir, formats @ ..] = args.as_slice() else {
        return Err("usage: web_export INPUT OUT_DIR FORMAT...".into());
    };
    let input = Path::new(input);
    let name = input
        .file_name()
        .ok_or("INPUT has no file name")?
        .to_string_lossy()
        .into_owned();
    let bytes = std::fs::read(input).map_err(|e| format!("{}: {e}", input.display()))?;
    let work = Path::new(out_dir).join(".work");
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let mut web = WebSession::new(&work);
    web.open(&name, bytes)?;
    for format in formats {
        for f in web.export(format)? {
            let out = Path::new(out_dir).join(format!(
                "{name}.{}",
                f.name.rsplit('.').next().unwrap_or(format)
            ));
            std::fs::write(&out, &f.bytes).map_err(|e| e.to_string())?;
            println!("{} {} bytes", out.display(), f.bytes.len());
        }
    }
    std::fs::remove_dir(&work).map_err(|e| e.to_string())
}
