//! Serves the playground on <http://localhost:8000>, to try it before it's
//! deployed: browsers won't run it from a `file://` URL.
//!
//! ```sh
//! cargo run --example localhost            # another port: -- 8080
//! ```
//!
//! It builds the wasm module first, then serves `web/` as it is on disk, so
//! changes to the page show on reload; restart it after changing the Rust
//! code. It only listens on 127.0.0.1.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fs, thread};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const WASM: &str = "cel_playground.wasm";
const TARGET: &str = "wasm32-unknown-unknown";

fn main() -> ExitCode {
    let port = match env::args().nth(1).map(|port| port.parse::<u16>()) {
        None => 8000,
        Some(Ok(port)) => port,
        Some(Err(_)) => {
            eprintln!("usage: cargo run --example localhost [-- <port>]");
            return ExitCode::FAILURE;
        }
    };
    let wasm = match build() {
        Ok(wasm) => wasm,
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
    let web = Path::new(ROOT).join("web");
    for stream in listener.incoming().flatten() {
        let (web, wasm) = (web.clone(), wasm.clone());
        thread::spawn(move || {
            if let Err(error) = serve(&stream, &web, &wasm) {
                eprintln!("{error}");
            }
        });
    }
    ExitCode::SUCCESS
}

/// Builds the module as `build.sh` does, and returns where it is.
fn build() -> Result<PathBuf, String> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(&cargo)
        .current_dir(ROOT)
        .args(["build", "--release", "--target", TARGET])
        .status()
        .map_err(|error| format!("can't run cargo: {error}"))?;
    if !status.success() {
        return Err(format!("building the wasm module failed: {status}"));
    }
    // Wherever CARGO_TARGET_DIR or a config put it
    let metadata = Command::new(&cargo)
        .current_dir(ROOT)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|error| format!("can't run cargo metadata: {error}"))?;
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout)
        .map_err(|error| format!("unexpected cargo metadata: {error}"))?;
    let target_directory = metadata["target_directory"]
        .as_str()
        .ok_or("cargo metadata has no target_directory")?;
    Ok(Path::new(target_directory)
        .join(TARGET)
        .join("release")
        .join(WASM))
}

fn serve(stream: &TcpStream, web: &Path, wasm: &Path) -> std::io::Result<()> {
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
        "GET" | "HEAD" => file(&path, web, wasm),
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

/// The file at `path`, and its content type: one directly in `web/`, or the
/// module. Nothing else, so no `..`, encoded or not, gets out of `web/`.
fn file(path: &str, web: &Path, wasm: &Path) -> Result<(Vec<u8>, &'static str), u16> {
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
    let file = if name == WASM {
        wasm.to_path_buf()
    } else {
        web.join(name)
    };
    fs::read(file)
        .map(|body| (body, content_type))
        .map_err(|_| 404)
}
