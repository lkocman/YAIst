//! yaist-helper: the privileged half of yAIst, started through pkexec.
//!
//! It runs one fixed operation per invocation and re-validates every argument
//! itself, since whoever starts it is not trusted. Multi-step operations (a
//! Leap upgrade) run under one authentication instead of one per step.
//! This is the seed of the planned `yaistd` daemon.
//!
//!   yaist-helper snapper-list
//!   yaist-helper snapper-create <description>
//!   yaist-helper snapper-status <number>
//!   yaist-helper snapper-delete <number>
//!   yaist-helper snapper-rollback <number>
//!   yaist-helper migrate-leap leap|leap-micro <version> [repo-alias-to-disable…]

use std::path::Path;
use std::process::{Command, ExitCode};

use yaist_core::validate;

const TMP_REPO: &str = "yaist-migration-tmp";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("yaist-helper: {e}");
            ExitCode::FAILURE
        }
    }
}

type R = Result<(), String>;

fn run(args: &[String]) -> R {
    let arg = |i: usize| args.get(i).map(String::as_str).ok_or_else(|| format!("missing argument {i}"));
    let number = |i: usize| -> Result<&str, String> {
        let n = arg(i)?;
        validate::snapshot_number(n)?;
        Ok(n)
    };
    match args.first().map(String::as_str) {
        Some("snapper-list") => snapper(&["--jsonout", "list", "--disable-used-space"]),
        Some("snapper-create") => {
            let d = arg(1)?;
            validate::description(d)?;
            snapper(&["create", "--description", d, "--cleanup-algorithm", "number", "--print-number", "--userdata", "important=yes"])
        }
        Some("snapper-status") => snapper(&["status", &format!("{}..0", number(1)?)]),
        Some("snapper-delete") => snapper(&["delete", number(1)?]),
        Some("snapper-rollback") => {
            let n = number(1)?;
            if Path::new("/usr/sbin/transactional-update").exists() && root_read_only() {
                exec("/usr/sbin/transactional-update", &["--non-interactive", "rollback", n])
            } else {
                snapper(&["rollback", n])
            }
        }
        Some("migrate-leap") => migrate_leap(arg(1)?, arg(2)?, &args[3..]),
        _ => Err("usage: yaist-helper snapper-list|snapper-create|snapper-status|snapper-delete|snapper-rollback|migrate-leap …".into()),
    }
}

fn exec(prog: &str, args: &[&str]) -> R {
    println!("==> {prog} {}", args.join(" "));
    let status = Command::new(prog)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("LC_ALL", "C.UTF-8")
        .status()
        .map_err(|e| format!("running {prog}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{prog} {} failed ({status})", args.first().unwrap_or(&"")))
    }
}

fn snapper(args: &[&str]) -> R {
    let mut full = vec!["-c", "root"];
    full.extend_from_slice(args);
    exec("/usr/bin/snapper", &full)
}

fn zypper(args: &[&str]) -> R {
    let mut full = vec!["--non-interactive"];
    full.extend_from_slice(args);
    exec("/usr/bin/zypper", &full)
}

fn root_read_only() -> bool {
    std::fs::read_to_string("/proc/mounts").unwrap_or_default().lines().any(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        f.len() > 3 && f[1] == "/" && f[3].split(',').any(|o| o == "ro")
    })
}

fn os_release(key: &str) -> String {
    std::fs::read_to_string("/etc/os-release")
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
        .unwrap_or("")
        .trim_matches('"')
        .to_owned()
}

/// Leap 16 needs x86-64-v2.
fn x86_64_v2() -> bool {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let flags: Vec<&str> = cpuinfo.lines().find(|l| l.starts_with("flags")).map(|l| l.split_whitespace().collect()).unwrap_or_default();
    ["cx16", "sse4_1", "sse4_2", "popcnt"].iter().all(|f| flags.contains(f))
}

/// Is this a transactional (immutable) system? Immutable Leap says so in
/// os-release; a read-only root catches the rest.
fn transactional() -> bool {
    os_release("VARIANT_ID") == "transactional"
        || os_release("ID_LIKE").split_whitespace().any(|l| l == "opensuse-microos")
        || ["opensuse-microos", "opensuse-aeon", "opensuse-kalpa", "opensuse-leap-micro"].contains(&os_release("ID").as_str())
        || (Path::new("/usr/sbin/transactional-update").exists() && root_read_only())
}

/// Upgrades Leap (or Leap Micro) to a newer release, the way
/// opensuse-migration-tool does: the repository definitions come from the
/// *target* release's openSUSE-repos package, then
/// `zypper --releasever <target> dup`. On transactional systems every step runs
/// inside one chain of snapshots (`transactional-update --continue run`), which
/// becomes active after a reboot.
fn migrate_leap(flavour: &str, target: &str, disable: &[String]) -> R {
    validate::release_version(target)?;
    let (id, repos_pkg) = match flavour {
        "leap" => ("opensuse-leap", "openSUSE-repos-Leap"),
        "leap-micro" => ("opensuse-leap-micro", "openSUSE-repos-LeapMicro"),
        _ => return Err(format!("unknown Leap flavour {flavour}")),
    };
    if os_release("ID") != id {
        return Err(format!("this system is not {id}"));
    }
    let current = os_release("VERSION_ID");
    if !validate::version_newer(target, &current) {
        return Err(format!("{target} is not newer than the installed {current}"));
    }
    let major: u32 = target.split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0);
    if flavour == "leap" && std::env::consts::ARCH == "x86_64" && major >= 16 && !x86_64_v2() {
        return Err(format!("Leap {target} needs an x86-64-v2 CPU; see https://en.opensuse.org/openSUSE:X86-64-Architecture-Levels"));
    }
    let url = match flavour {
        "leap" => format!("https://download.opensuse.org/distribution/leap/{target}/repo/oss"),
        _ => format!(
            "https://download.opensuse.org/distribution/leap-micro/{target}/product/repo/openSUSE-Leap-Micro-{target}-{}/",
            std::env::consts::ARCH
        ),
    };

    let tx = transactional();
    let step = |args: &[&str]| -> R {
        if tx {
            let mut full = vec!["--non-interactive", "--continue", "run", "/usr/bin/zypper", "--non-interactive"];
            full.extend_from_slice(args);
            exec("/usr/sbin/transactional-update", &full)
        } else {
            zypper(args)
        }
    };

    // Third-party repositories pinned to the old release break `zypper dup`.
    for alias in disable {
        validate::name(alias)?;
        if step(&["modifyrepo", "--disable", "--", alias]).is_err() {
            println!("    (repository {alias} not found, skipped)");
        }
    }
    step(&["install", "--force-resolution", repos_pkg])?;
    step(&["refresh-services"])?;
    step(&["addrepo", "--refresh", "--", &url, TMP_REPO])?;
    let installed = step(&["install", "--repo", TMP_REPO, "--force-resolution", "--from", TMP_REPO, repos_pkg]);
    let _ = step(&["removerepo", "--", TMP_REPO]);
    installed?;
    step(&["refresh-services"])?;
    // --releasever is a global option: it goes before the command.
    step(&["--releasever", target, "dup", "--force-resolution", "--allow-vendor-change", "--download-in-advance"])?;
    println!("==> upgraded to {target}; reboot to finish");
    Ok(())
}
