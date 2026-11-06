//! `openstitch-thumbnailer`: writes a PNG thumbnail of an OpenStitch project or a machine
//! embroidery file, in the freedesktop thumbnailer calling convention.
//!
//! ```text
//! openstitch-thumbnailer [-s SIZE] INPUT OUTPUT.png
//! openstitch-thumbnailer [-s SIZE] --stdin EXTENSION   < file > out.png
//! ```
//! Installed with `share/thumbnailers/openstitch.thumbnailer`
//! (`Exec=openstitch-thumbnailer -s %s %i %o`), so GNOME, KDE and other freedesktop file
//! managers show stitched previews of `.osd` and `.pes` files. `--stdin` reads the file's
//! bytes from standard input (its format named by EXTENSION, such as `pes` or `osd`) and writes
//! the PNG to standard output: the macOS Quick Look extension (packaging/macos/quicklook) runs
//! it that way, since a sandboxed helper cannot open the user's file by path (#557).

use openstitch_render::thumbnail;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: openstitch-thumbnailer [-s SIZE] INPUT OUTPUT.png
       openstitch-thumbnailer [-s SIZE] --stdin EXTENSION  < INPUT > OUTPUT.png";

/// Where the file comes from and the PNG goes.
#[derive(Debug, PartialEq)]
enum Io {
    Files(PathBuf, PathBuf),
    /// Bytes on stdin in the format of this extension, PNG on stdout.
    Pipe(String),
}

fn parse(args: &[String]) -> Result<(u32, Io), String> {
    let mut size = 256;
    let mut files = Vec::new();
    let mut stdin = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-s" | "--size" => {
                size = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .filter(|n| (1..=4096).contains(n))
                    .ok_or("-s takes a size in pixels")?;
            }
            "--stdin" => {
                stdin = Some(
                    it.next()
                        .filter(|e| !e.is_empty() && !e.starts_with('-'))
                        .ok_or("--stdin takes the file's extension")?
                        .trim_start_matches('.')
                        .to_string(),
                );
            }
            "-h" | "--help" => return Err(String::new()),
            _ => files.push(PathBuf::from(a)),
        }
    }
    match (stdin, <[PathBuf; 2]>::try_from(files)) {
        (None, Ok([input, output])) => Ok((size, Io::Files(input, output))),
        (Some(ext), Err(rest)) if rest.is_empty() => Ok((size, Io::Pipe(ext))),
        (Some(_), _) => Err("--stdin takes no INPUT or OUTPUT".into()),
        (None, Err(_)) => Err("expected INPUT and OUTPUT".into()),
    }
}

fn run(size: u32, io: Io) -> Result<(), String> {
    match io {
        Io::Files(input, output) => {
            let png = thumbnail::load(&input).and_then(|p| thumbnail::png(&p, size))?;
            std::fs::write(&output, png).map_err(|e| format!("{}: {e}", output.display()))
        }
        Io::Pipe(ext) => {
            let mut data = Vec::new();
            std::io::stdin()
                .take(thumbnail::MAX_INPUT + 1)
                .read_to_end(&mut data)
                .map_err(|e| format!("stdin: {e}"))?;
            let png = thumbnail::load_bytes(&data, &ext).and_then(|p| thumbnail::png(&p, size))?;
            let mut out = std::io::stdout().lock();
            out.write_all(&png)
                .and_then(|()| out.flush())
                .map_err(|e| format!("stdout: {e}"))
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (size, io) = match parse(&args) {
        Ok(v) => v,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("openstitch-thumbnailer: {e}");
            }
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(size, io) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("openstitch-thumbnailer: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Io, parse};
    use std::path::PathBuf;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parses_freedesktop_arguments() {
        let (size, io) = parse(&args("-s 128 in.pes out.png")).expect("ok");
        assert_eq!(
            (size, io),
            (
                128,
                Io::Files(PathBuf::from("in.pes"), PathBuf::from("out.png"))
            )
        );
        assert_eq!(parse(&args("a.osd b.png")).expect("ok").0, 256);
        assert!(parse(&args("-s 0 a b")).is_err());
        assert!(parse(&args("-s x a b")).is_err());
        assert!(parse(&args("only-one")).is_err());
        assert!(parse(&args("a b c")).is_err());
    }

    #[test]
    fn parses_the_quick_look_pipe() {
        let (size, io) = parse(&args("-s 512 --stdin .PES")).expect("ok");
        assert_eq!((size, io), (512, Io::Pipe("PES".into())));
        assert!(parse(&args("--stdin")).is_err());
        assert!(parse(&args("--stdin -s 5")).is_err());
        assert!(parse(&args("--stdin pes in.pes")).is_err());
    }
}
