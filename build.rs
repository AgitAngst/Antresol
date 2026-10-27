//! Значок `assets/icon/antresol.ico` — ресурс exe (файлы собирает `scripts/render-icon.ps1`); номер сборки — число
//! коммитов, для «О программе» (`ANTRESOL_BUILD`; нет git — пусто).

use std::path::PathBuf;
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    let ico = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("assets/icon/antresol.ico");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", ico.display());

    // Новый коммит — новый номер: следим за журналом HEAD (в worktree путь другой, его знает git).
    if let Some(log) = git(&["rev-parse", "--git-path", "logs/HEAD"]) {
        println!("cargo:rerun-if-changed={log}");
    }
    let build = git(&["rev-list", "--count", "HEAD"]).unwrap_or_default();
    println!("cargo:rustc-env=ANTRESOL_BUILD={build}");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon(ico.to_str().expect("utf-8 path"));
    res.set("ProductName", "Antresol");
    res.set("FileDescription", "Antresol — quick-access panel at the top edge of the monitor");
    if let Err(e) = res.compile() {
        // Без rc.exe программа соберётся, просто без значка у exe.
        println!("cargo:warning=exe icon skipped: {e}");
    }
}
