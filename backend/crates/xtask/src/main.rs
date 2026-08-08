//! Dev-workflow automation, invoked as `cargo xtask <command>` (alias defined
//! in `.cargo/config.toml` at the repo root). Wraps the cargo/npm commands
//! documented in the README so "build/test/run everything" is one command
//! instead of remembering which tool to run in which directory.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::thread;

use anyhow::{bail, Context, Result};

struct Paths {
    backend: PathBuf,
    frontend: PathBuf,
}

/// `xtask` lives at `backend/crates/xtask`, so the backend workspace root and
/// the sibling `frontend/` directory are both fixed offsets from
/// `CARGO_MANIFEST_DIR`.
fn paths() -> Paths {
    let xtask_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let backend = xtask_dir
        .parent()
        .and_then(Path::parent)
        .expect("xtask crate should be nested two levels under backend/")
        .to_path_buf();
    let repo_root = backend
        .parent()
        .expect("backend/ should be nested one level under the repo root")
        .to_path_buf();
    let frontend = repo_root.join("frontend");
    Paths { backend, frontend }
}

fn main() -> Result<()> {
    let command = std::env::args().nth(1);
    let paths = paths();

    match command.as_deref() {
        Some("build") => build(&paths),
        Some("test") => test(&paths),
        Some("lint") => lint(&paths),
        Some("release") => release(&paths).map(|_| ()),
        Some("run") => run_release(&paths),
        Some("dev") => dev(&paths),
        Some("ci") => ci(&paths),
        Some(other) => {
            eprintln!("unknown xtask command: {other}\n");
            print_help();
            std::process::exit(1);
        }
        None => {
            print_help();
            Ok(())
        }
    }
}

fn print_help() {
    println!(
        "cargo xtask <command>\n\n\
         Commands:\n\
         \x20 build    Build the frontend (npm run build) and the backend workspace (cargo build --workspace)\n\
         \x20 test     Run the backend test suite (cargo test --workspace)\n\
         \x20 lint     Run clippy (default + serve-frontend features) and check formatting\n\
         \x20 release  Build the frontend, then the backend with the frontend embedded into one binary\n\
         \x20 run      release, then execute the resulting single-binary server (serves API + SPA on :8080)\n\
         \x20 dev      Run the backend (:8080) and frontend (:5173) dev servers together, for local hot-reload\n\
         \x20 ci       test + lint + build -- everything that should be clean before committing"
    );
}

/// Runs `program args...` in `dir`, streaming stdout/stderr straight through,
/// and turns a non-zero exit into an `Err` so callers can just use `?`.
fn run_in(dir: &Path, program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .with_context(|| format!("failed to spawn `{program} {}`", args.join(" ")))?;
    if !status.success() {
        bail!(
            "`{program} {}` (in {}) exited with {status}",
            args.join(" "),
            dir.display()
        );
    }
    Ok(())
}

fn ensure_frontend_deps(paths: &Paths) -> Result<()> {
    if !paths.frontend.join("node_modules").is_dir() {
        run_in(&paths.frontend, "npm", &["install"])?;
    }
    Ok(())
}

fn frontend_build(paths: &Paths) -> Result<()> {
    ensure_frontend_deps(paths)?;
    run_in(&paths.frontend, "npm", &["run", "build"])
}

fn build(paths: &Paths) -> Result<()> {
    frontend_build(paths)?;
    run_in(&paths.backend, "cargo", &["build", "--workspace"])
}

fn test(paths: &Paths) -> Result<()> {
    run_in(&paths.backend, "cargo", &["test", "--workspace"])
}

fn lint(paths: &Paths) -> Result<()> {
    run_in(
        &paths.backend,
        "cargo",
        &["clippy", "--workspace", "--all-targets"],
    )?;
    run_in(
        &paths.backend,
        "cargo",
        &[
            "clippy",
            "-p",
            "usmf-api",
            "--all-targets",
            "--features",
            "serve-frontend",
        ],
    )?;
    run_in(&paths.backend, "cargo", &["fmt", "--all", "--", "--check"])
}

fn ci(paths: &Paths) -> Result<()> {
    test(paths)?;
    lint(paths)?;
    build(paths)
}

/// Builds the frontend and the single embedded release binary, returning its
/// path so `run` can execute it directly.
fn release(paths: &Paths) -> Result<PathBuf> {
    frontend_build(paths)?;
    run_in(
        &paths.backend,
        "cargo",
        &[
            "build",
            "--release",
            "-p",
            "usmf-api",
            "--features",
            "serve-frontend",
        ],
    )?;
    Ok(paths.backend.join("target/release/usmf-api"))
}

fn run_release(paths: &Paths) -> Result<()> {
    let binary = release(paths)?;
    let status = Command::new(&binary)
        .current_dir(&paths.backend)
        .status()
        .with_context(|| format!("failed to run {}", binary.display()))?;
    if !status.success() {
        bail!("{} exited with {status}", binary.display());
    }
    Ok(())
}

/// Runs the backend and frontend dev servers side by side. Both are spawned
/// as normal foreground children (same process group), so Ctrl+C in the
/// terminal reaches them directly without any signal-forwarding here -- this
/// just waits for both and surfaces whichever one failed.
fn dev(paths: &Paths) -> Result<()> {
    ensure_frontend_deps(paths)?;

    let backend_dir = paths.backend.clone();
    let backend = thread::spawn(move || -> Result<ExitStatus> {
        Command::new("cargo")
            .args(["run", "-p", "usmf-api"])
            .current_dir(&backend_dir)
            .status()
            .context("failed to spawn `cargo run -p usmf-api`")
    });

    let frontend_dir = paths.frontend.clone();
    let frontend = thread::spawn(move || -> Result<ExitStatus> {
        Command::new("npm")
            .args(["run", "dev"])
            .current_dir(&frontend_dir)
            .status()
            .context("failed to spawn `npm run dev`")
    });

    let backend_status = backend.join().expect("backend thread panicked")?;
    let frontend_status = frontend.join().expect("frontend thread panicked")?;

    if !backend_status.success() {
        bail!("backend dev server exited with {backend_status}");
    }
    if !frontend_status.success() {
        bail!("frontend dev server exited with {frontend_status}");
    }
    Ok(())
}
