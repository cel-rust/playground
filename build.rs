//! Embeds what the page shows about this build: the version of `cel` it
//! resolved from `Cargo.lock`, and the playground commit it was built from.

use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rustc-env=CEL_VERSION={}", cel_version());
    println!("cargo:rustc-env=PLAYGROUND_COMMIT={}", playground_commit());
}

fn cel_version() -> String {
    println!("cargo:rerun-if-changed=Cargo.lock");
    let lock = fs::read_to_string("Cargo.lock").unwrap_or_default();
    lock.split("[[package]]")
        .find_map(|package| {
            let mut lines = package.lines().map(str::trim);
            lines.find(|line| *line == r#"name = "cel""#)?;
            lines
                .find_map(|line| line.strip_prefix("version = "))
                .map(|version| version.trim_matches('"').to_owned())
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

/// The commit GitHub Actions builds, or the checkout's `HEAD`: uncommitted
/// changes don't show.
fn playground_commit() -> String {
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    if let Some(sha) = std::env::var("GITHUB_SHA")
        .ok()
        .filter(|sha| !sha.is_empty())
    {
        return sha;
    }
    // Run again when HEAD moves: HEAD itself, or the branch it is on
    if let Some(git_dir) = git(&["rev-parse", "--git-dir"]) {
        let git_dir = Path::new(&git_dir);
        let branch = git(&["symbolic-ref", "-q", "HEAD"]);
        for file in ["HEAD", "packed-refs"].into_iter().chain(branch.as_deref()) {
            let path = git_dir.join(file);
            if path.exists() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
    git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_owned())
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let text = text.trim();
    (output.status.success() && !text.is_empty()).then(|| text.to_owned())
}
