//! Serves the playground on <http://localhost:8000>, to try it before it's
//! deployed: browsers won't run it from a `file://` URL.
//!
//! ```sh
//! cargo run --example localhost            # another port: -- 8080
//! ```
//!
//! It builds a wasm module for each version of cel first, as `build.sh`
//! does, then serves `web/` as it is on disk, so changes to the page show on
//! reload; restart it after changing the Rust code. It only listens on
//! 127.0.0.1.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::Arc;
use std::{env, fs, thread};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const WASM: &str = "cel_playground.wasm";
const TARGET: &str = "wasm32-unknown-unknown";

/// What's served: the files directly in `web/`, and what `build.sh` adds.
struct Site {
    web: PathBuf,
    /// Each version of cel on offer, newest first, and its module.
    modules: Vec<(String, Vec<u8>)>,
}

fn main() -> ExitCode {
    let port = match env::args().nth(1).map(|port| port.parse::<u16>()) {
        None => 8000,
        Some(Ok(port)) => port,
        Some(Err(_)) => {
            eprintln!("usage: cargo run --example localhost [-- <port>]");
            return ExitCode::FAILURE;
        }
    };
    let modules = match build() {
        Ok(modules) => modules,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("can't listen on port {port}: {error}");
            return ExitCode::FAILURE;
        }
    };

    println!("serving the playground on http://localhost:{port}/ (Ctrl+C to stop)");
    let site = Arc::new(Site {
        web: Path::new(ROOT).join("web"),
        modules,
    });
    for stream in listener.incoming().flatten() {
        let site = Arc::clone(&site);
        thread::spawn(move || {
            if let Err(error) = serve(&stream, &site) {
                eprintln!("{error}");
            }
        });
    }
    ExitCode::SUCCESS
}

/// Builds a module for each version of cel, newest first, as `build.sh` does:
/// one per `cel-0-N` feature, with the version of the 0.N line in
/// `Cargo.lock` (see `versions.sh`).
fn build() -> Result<Vec<(String, Vec<u8>)>, String> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let run = |args: &[&str]| {
        let output = Command::new(&cargo)
            .current_dir(ROOT)
            .args(args)
            .stderr(Stdio::inherit())
            .output()
            .map_err(|error| format!("can't run cargo: {error}"))?;
        if !output.status.success() {
            let command = args.join(" ");
            return Err(format!("cargo {command} failed: {}", output.status));
        }
        String::from_utf8(output.stdout).map_err(|error| format!("cargo {}: {error}", args[0]))
    };

    let metadata = run(&["metadata", "--format-version", "1", "--no-deps"])?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata)
        .map_err(|error| format!("unexpected cargo metadata: {error}"))?;
    // Wherever CARGO_TARGET_DIR or a config put it
    let target_directory = metadata["target_directory"]
        .as_str()
        .ok_or("cargo metadata has no target_directory")?;
    let wasm = Path::new(target_directory)
        .join(TARGET)
        .join("release")
        .join(WASM);
    // cel-0-16, of the 0.16 line, first
    let mut lines: Vec<(&str, Vec<u32>)> = metadata["packages"][0]["features"]
        .as_object()
        .ok_or("cargo metadata has no features")?
        .keys()
        .filter_map(|feature| {
            let numbers = feature.strip_prefix("cel-")?.split('-');
            let line = numbers.map(|n| n.parse().ok()).collect::<Option<_>>()?;
            Some((feature.as_str(), line))
        })
        .collect();
    lines.sort_by(|a, b| b.1.cmp(&a.1));
    if lines.is_empty() {
        return Err("no cel-0-N features in Cargo.toml".to_owned());
    }

    let mut modules = Vec::new();
    for (feature, _) in lines {
        let line = feature["cel-".len()..].replace('-', ".");
        let id = run(&["pkgid", "--quiet", &format!("cel@{line}")])?;
        let version = id.trim().rsplit_once('@').map_or("unknown", |(_, v)| v);
        let features = ["--no-default-features", "--features", feature];
        run(&[&["build", "--release", "--target", TARGET], &features[..]].concat())?;
        let module = fs::read(&wasm).map_err(|error| format!("{}: {error}", wasm.display()))?;
        println!("built cel-{version}.wasm ({} bytes)", module.len());
        modules.push((version.to_owned(), module));
    }
    Ok(modules)
}

fn serve(stream: &TcpStream, site: &Site) -> std::io::Result<()> {
    let mut request = BufReader::new(stream.take(64 * 1024));
    let mut line = String::new();
    request.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or_default();
    let path = path.split(['?', '#']).next().unwrap_or_default().to_owned();
    // The headers: nothing here needs them
    loop {
        line.clear();
        if request.read_line(&mut line)? <= 2 {
            break;
        }
    }

    let response = match method.as_str() {
        "GET" | "HEAD" => file(&path, site),
        _ => Err(405),
    };
    let mut out = stream;
    match response {
        Ok((body, content_type)) => {
            println!("200 {method} {path}");
            write!(
                out,
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
                 Cache-Control: no-store\r\nConnection: close\r\n\r\n",
                body.len()
            )?;
            if method == "GET" {
                out.write_all(&body)?;
            }
        }
        Err(status) => {
            println!("{status} {method} {path}");
            let reason = match status {
                404 => "Not Found",
                _ => "Method Not Allowed",
            };
            write!(
                out,
                "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{reason}\n",
                reason.len() + 1
            )?;
        }
    }
    out.flush()
}

/// The file at `path`, and its content type: one directly in `web/`, a module,
/// or the list of their versions. Nothing else, so no `..`, encoded or not,
/// gets out of `web/`.
fn file(path: &str, site: &Site) -> Result<(Vec<u8>, &'static str), u16> {
    let name = match path.strip_prefix('/') {
        Some("") => "index.html",
        Some(name) => name,
        None => return Err(404),
    };
    let allowed = |b: u8| b.is_ascii_alphanumeric() || b"._-".contains(&b);
    if name.starts_with('.') || !name.bytes().all(allowed) {
        return Err(404);
    }
    let content_type = match name.rsplit_once('.').map(|(_, extension)| extension) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    };
    if name == "versions.json" {
        let versions = site.modules.iter().map(|(version, _)| version.as_str());
        let body = serde_json::Value::from_iter(versions).to_string();
        return Ok((body.into_bytes(), content_type));
    }
    if let Some((_, module)) = site
        .modules
        .iter()
        .find(|(version, _)| name == format!("cel-{version}.wasm"))
    {
        return Ok((module.clone(), content_type));
    }
    fs::read(site.web.join(name))
        .map(|body| (body, content_type))
        .map_err(|_| 404)
}
