//! Reads the version of `cel` this build resolved from `Cargo.lock`, so the
//! page can show it and put it in bug reports.

use std::fs;

fn main() {
    println!("cargo:rerun-if-changed=Cargo.lock");
    let lock = fs::read_to_string("Cargo.lock").unwrap_or_default();
    let version = lock
        .split("[[package]]")
        .find_map(|package| {
            let mut lines = package.lines().map(str::trim);
            lines.find(|line| *line == r#"name = "cel""#)?;
            lines
                .find_map(|line| line.strip_prefix("version = "))
                .map(|version| version.trim_matches('"').to_owned())
        })
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=CEL_VERSION={version}");
}
