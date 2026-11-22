//! `cargo xtask <task>` — the mandatory checks from AGENTS.md §3, in one place.
//!
//! - `check`: fmt, clippy -D warnings, tests, rt-check, miri (when unsafe code exists),
//!   cargo-deny, wasm-check, and the preset/wasm suites once their crates exist
//! - `wasm-check`: `cargo check --target wasm32-unknown-unknown` for the wasm-portable crates
//! - `rt-check`: the no-allocation tests for the audio thread (debug build, assert_no_alloc)
//! - `bench`: release build checked against the lightweight targets (startup, memory, binary
//!   size, 24-track DSP load); see bench.rs
//! - `status`: counts the working menu entries and roadmap items for the README; see status.rs
mod bench;
mod package_installers;
mod status;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Crates that must build for the browser (AGENTS.md §3.5: DSP parity on wasm32).
const WASM_CRATES: &[&str] = &["prowave-engine"];
/// Crates whose `unsafe` code must pass miri (AGENTS.md §3).
const MIRI_CRATES: &[&str] = &["prowave-engine", "prowave-dsp", "prowave-instruments"];

fn root() -> PathBuf {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    here.parent().unwrap_or(here).to_path_buf()
}

/// Project-local tool installs (`cargo install --root target/tools-root ...`) are put first on
/// PATH, so tools like cargo-deny work without writing to ~/.cargo/bin.
fn cmd(program: &str) -> Command {
    let mut c = Command::new(program);
    let local = root().join("target").join("tools-root").join("bin");
    if local.is_dir() {
        let mut paths = vec![local];
        if let Some(p) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&p));
        }
        if let Ok(joined) = std::env::join_paths(paths) {
            c.env("PATH", joined);
        }
    }
    c.current_dir(root());
    c
}

fn crate_exists(name: &str) -> bool {
    root()
        .join("crates")
        .join(name)
        .join("Cargo.toml")
        .is_file()
}

struct Step<'a> {
    name: &'a str,
    program: &'a str,
    args: Vec<&'a str>,
    /// Extra environment variables for this step.
    env: Vec<(&'a str, &'a str)>,
}

fn step<'a>(name: &'a str, program: &'a str, args: &[&'a str]) -> Step<'a> {
    Step {
        name,
        program,
        args: args.to_vec(),
        env: Vec::new(),
    }
}

fn run(s: &Step) -> bool {
    println!("\n==> {} ({} {})", s.name, s.program, s.args.join(" "));
    match cmd(s.program)
        .args(&s.args)
        .envs(s.env.iter().copied())
        .status()
    {
        Ok(st) if st.success() => true,
        Ok(st) => {
            eprintln!("xtask: `{}` failed ({st})", s.name);
            false
        }
        Err(e) => {
            eprintln!("xtask: cannot run `{}`: {e}", s.program);
            false
        }
    }
}

fn tool_available(program: &str, args: &[&str]) -> bool {
    cmd(program)
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success())
}

/// True if any `.rs` file under `crates/<name>/src` contains an `unsafe` block or impl.
fn has_unsafe(name: &str) -> bool {
    fn scan(dir: &Path) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        entries.flatten().any(|e| {
            let p = e.path();
            if p.is_dir() {
                scan(&p)
            } else if p.extension().is_some_and(|x| x == "rs") {
                std::fs::read_to_string(&p).is_ok_and(|src| {
                    src.lines()
                        .filter(|l| !l.trim_start().starts_with("//"))
                        .any(|l| {
                            l.contains("unsafe {")
                                || l.contains("unsafe impl")
                                || l.contains("unsafe fn")
                        })
                })
            } else {
                false
            }
        })
    }
    scan(&root().join("crates").join(name).join("src"))
}

fn wasm_steps() -> Vec<Step<'static>> {
    WASM_CRATES
        .iter()
        .filter(|c| crate_exists(c))
        .map(|c| {
            step(
                "wasm-check",
                "cargo",
                &["check", "-p", c, "--target", "wasm32-unknown-unknown"],
            )
        })
        .collect()
}

fn miri_steps() -> Vec<Step<'static>> {
    let unsafe_crates: Vec<&str> = MIRI_CRATES
        .iter()
        .copied()
        .filter(|c| crate_exists(c) && has_unsafe(c))
        .collect();
    if unsafe_crates.is_empty() {
        println!("xtask: no `unsafe` code in {MIRI_CRATES:?}; miri skipped");
        return Vec::new();
    }
    let mut steps = Vec::new();
    for c in unsafe_crates {
        steps.push(Step {
            name: "miri",
            program: "cargo",
            args: vec!["+nightly", "miri", "test", "-p", c, "--lib", "triple"],
            env: if cfg!(windows) {
                vec![("MIRIFLAGS", "-Zmiri-disable-stacked-borrows")]
            } else {
                Vec::new()
            },
        });
    }
    steps
}

fn rt_step() -> Step<'static> {
    // Tests named `*does_not_allocate` wrap `process()` in `assert_no_alloc` (debug only).
    step(
        "rt-check",
        "cargo",
        &[
            "test",
            "--workspace",
            "--",
            "does_not_allocate",
            "--nocapture",
        ],
    )
}

fn test_step() -> Step<'static> {
    if tool_available("cargo", &["nextest", "--version"]) {
        step(
            "test",
            "cargo",
            &["nextest", "run", "--workspace", "--lib", "--bins"],
        )
    } else {
        step(
            "test",
            "cargo",
            &[
                "test",
                "--workspace",
                "--lib",
                "--bins",
                "--",
                "--nocapture",
            ],
        )
    }
}

fn heavy_test_step() -> Step<'static> {
    if tool_available("cargo", &["nextest", "--version"]) {
        step(
            "heavy-tests",
            "cargo",
            &[
                "nextest",
                "run",
                "--workspace",
                "--run-ignored",
                "ignored-only",
            ],
        )
    } else {
        step(
            "heavy-tests",
            "cargo",
            &["test", "--workspace", "--", "--ignored", "--nocapture"],
        )
    }
}

fn test_crate(c: &str) -> bool {
    let s = if tool_available("cargo", &["nextest", "--version"]) {
        step(
            "test-crate",
            "cargo",
            &["nextest", "run", "-p", c, "--lib", "--bins"],
        )
    } else {
        step(
            "test-crate",
            "cargo",
            &["test", "-p", c, "--lib", "--bins", "--", "--nocapture"],
        )
    };
    run(&s)
}

fn check(full: bool) -> bool {
    let mut steps = vec![
        step("fmt", "cargo", &["fmt", "--all", "--check"]),
        step(
            "clippy",
            "cargo",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
        test_step(),
        rt_step(),
    ];

    if full {
        println!("xtask: running full verification suite (--full)");
        steps.push(heavy_test_step());
        steps.extend(miri_steps());

        if tool_available("cargo", &["deny", "--version"]) {
            steps.push(step(
                "deny",
                "cargo",
                &["deny", "check", "advisories", "bans", "licenses", "sources"],
            ));
        } else {
            println!("xtask: cargo-deny not installed; advisory/license check skipped");
        }

        steps.extend(wasm_steps());

        if crate_exists("prowave-dsp") {
            steps.push(step(
                "preset_compat",
                "cargo",
                &[
                    "test",
                    "-p",
                    "prowave-dsp",
                    "--test",
                    "preset_compat",
                    "--",
                    "--nocapture",
                ],
            ));
        } else {
            println!("xtask: prowave-dsp not created yet; preset_compat skipped");
        }
        if crate_exists("prowave-wasm") {
            steps.push(Step {
                name: "wasm-pack",
                program: "wasm-pack",
                args: vec!["test", "--node", "crates/prowave-wasm"],
                env: Vec::new(),
            });
        } else {
            println!("xtask: prowave-wasm not created yet; wasm-pack test skipped");
        }
    } else {
        println!(
            "xtask: fast daily gates (use --full for miri, wasm, cargo-deny, and heavy tests)"
        );
    }

    let mut failed = Vec::new();
    for s in &steps {
        if !run(s) {
            failed.push(s.name);
        }
    }
    if failed.is_empty() {
        println!("\nxtask check: all {} steps passed", steps.len());
        true
    } else {
        eprintln!("\nxtask check: FAILED {failed:?}");
        false
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let task = args.next().unwrap_or_default();
    let ok = match task.as_str() {
        "check" => {
            let full = args.any(|a| a == "--full" || a == "-f");
            check(full)
        }
        "miri" => {
            let steps = miri_steps();
            if steps.is_empty() {
                true
            } else {
                steps.iter().all(run)
            }
        }
        "wasm" | "wasm-check" => wasm_steps().iter().all(run),
        "rt-check" => run(&rt_step()),
        "test-crate" => {
            if let Some(crate_name) = args.next() {
                test_crate(&crate_name)
            } else {
                eprintln!("usage: cargo xtask test-crate <crate_name>");
                false
            }
        }
        "bench" => bench::bench(&root()),
        "status" => match status::run(&root()) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("xtask status: {e}");
                false
            }
        },
        "package-installers" => package_installers::run(&root()),
        "package-rpm" => package_installers::build_rpm(&root()),
        _ => {
            eprintln!("usage: cargo xtask <check [--full]|miri|wasm|rt-check|test-crate <crate>|bench|status|package-installers|package-rpm>");
            false
        }
    };
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
