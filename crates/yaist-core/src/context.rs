//! What actions get to work with: the detected distribution, the system bus
//! and a way to run privileged helpers.
//!
//! Prototype note: privileged commands go through `pkexec` from the user's
//! process. In the real design they move into the `yaistd` daemon, behind
//! per-action polkit policy (see PLAN.md, "Security model").

use std::path::Path;
use std::process::Stdio;

use anyhow::{anyhow, bail, Context as _, Result};
use serde::Serialize;
use tokio::sync::OnceCell;
use zbus::zvariant::{DynamicDeserialize, DynamicType, OwnedValue};

/// The openSUSE flavour we're running on; actions adapt to it.
#[derive(Clone, Debug, Default)]
pub struct Distro {
    /// `ID` from os-release, e.g. `opensuse-leap`, `opensuse-tumbleweed`.
    pub id: String,
    pub version: String,
    pub pretty: String,
    /// Tumbleweed, Slowroll and the immutable variants roll; updating them
    /// means `zypper dup`, not `zypper up`.
    pub rolling: bool,
    /// Read-only root managed by `transactional-update` (MicroOS, Aeon,
    /// Kalpa, Leap Micro).
    pub transactional: bool,
}

impl Distro {
    pub fn detect() -> Self {
        let text = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
        let field =
            |key: &str| text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('=')).map(|v| v.trim_matches('"').to_owned()).unwrap_or_default();
        let id = field("ID");
        let rolling = ["tumbleweed", "slowroll", "microos", "aeon", "kalpa"].iter().any(|r| id.contains(r));
        let transactional = Path::new("/usr/sbin/transactional-update").exists() && root_is_read_only();
        Self { pretty: field("PRETTY_NAME"), version: field("VERSION_ID"), id, rolling, transactional }
    }

    pub fn is_opensuse(&self) -> bool {
        self.id.starts_with("opensuse") || self.id == "sles" || self.id == "sle-micro"
    }

    /// The path segment OBS uses for this distribution's repositories,
    /// e.g. `openSUSE_Tumbleweed` or `16.0`.
    pub fn obs_repo_name(&self) -> String {
        if self.id.contains("slowroll") {
            "openSUSE_Slowroll".into()
        } else if self.rolling {
            "openSUSE_Tumbleweed".into()
        } else {
            self.version.clone()
        }
    }
}

fn root_is_read_only() -> bool {
    std::fs::read_to_string("/proc/mounts").unwrap_or_default().lines().any(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        f.len() > 3 && f[1] == "/" && f[3].split(',').any(|o| o == "ro")
    })
}

fn effective_uid() -> Option<u32> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("Uid:"))?;
    line.split_whitespace().nth(2)?.parse().ok()
}

pub struct Context {
    pub distro: Distro,
    bus: OnceCell<zbus::Connection>,
}

impl Context {
    pub fn new() -> Self {
        Self { distro: Distro::detect(), bus: OnceCell::new() }
    }

    pub async fn system_bus(&self) -> Result<&zbus::Connection> {
        self.bus.get_or_try_init(|| async { zbus::Connection::system().await.context("connecting to the system D-Bus") }).await
    }

    async fn proxy(&self, dest: &'static str, path: &str, iface: &'static str) -> Result<zbus::Proxy<'static>> {
        let conn = self.system_bus().await?;
        Ok(zbus::proxy::Builder::<zbus::Proxy<'static>>::new(conn)
            .destination(dest)?
            .path(path.to_owned())?
            .interface(iface)?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await?)
    }

    /// Calls a D-Bus method. `interactive` lets the service ask polkit to
    /// authenticate the user (the desktop's polkit agent shows the dialog).
    pub async fn call<B, R>(
        &self,
        dest: &'static str,
        path: &str,
        iface: &'static str,
        method: &'static str,
        body: &B,
        interactive: bool,
    ) -> Result<R>
    where
        B: Serialize + DynamicType,
        R: for<'d> DynamicDeserialize<'d>,
    {
        let proxy = self.proxy(dest, path, iface).await?;
        let flags = if interactive { zbus::proxy::MethodFlags::AllowInteractiveAuth.into() } else { Default::default() };
        let reply = proxy.call_with_flags::<_, B, R>(method, flags, body).await.map_err(dbus_error)?;
        reply.ok_or_else(|| anyhow!("{method}: no reply"))
    }

    pub async fn property<T>(&self, dest: &'static str, path: &str, iface: &'static str, name: &'static str) -> Result<T>
    where
        T: TryFrom<OwnedValue>,
        T::Error: Into<zbus::Error>,
    {
        let proxy = self.proxy(dest, path, iface).await?;
        proxy.get_property::<T>(name).await.map_err(dbus_error)
    }

    /// Runs a command as the user. Output is forced to the C locale so it can
    /// be parsed.
    pub async fn run(&self, argv: &[&str]) -> Result<String> {
        run_command(argv, &[0]).await
    }

    /// Like [`Context::run`], treating the exit codes in `ok` as success
    /// (zypper reports "nothing found" or "some repos skipped" this way).
    pub async fn run_ok(&self, argv: &[&str], ok: &[i32]) -> Result<String> {
        run_command(argv, ok).await
    }

    /// Runs a command as root: directly when we already are, otherwise via
    /// pkexec. Arguments are passed as argv, never through a shell.
    pub async fn run_privileged(&self, argv: &[&str]) -> Result<String> {
        self.run_privileged_ok(argv, &[0]).await
    }

    pub async fn run_privileged_ok(&self, argv: &[&str], ok: &[i32]) -> Result<String> {
        if effective_uid() == Some(0) {
            return run_command(argv, ok).await;
        }
        let mut full = vec!["/usr/bin/pkexec"];
        full.extend_from_slice(argv);
        run_command(&full, ok).await.map_err(|e| {
            let msg = e.to_string();
            if msg.contains("exit status: 126") {
                anyhow!("authentication was cancelled")
            } else if msg.contains("exit status: 127") {
                anyhow!("not authorized (no polkit agent? run yaist as root, or from a desktop session)")
            } else {
                e
            }
        })
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

async fn run_command(argv: &[&str], ok: &[i32]) -> Result<String> {
    let (prog, rest) = argv.split_first().ok_or_else(|| anyhow!("empty command"))?;
    let out = tokio::process::Command::new(prog)
        .args(rest)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("LC_ALL", "C.UTF-8")
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .with_context(|| format!("running {prog}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.code().is_some_and(|c| ok.contains(&c)) {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let detail = if stderr.trim().is_empty() { stdout.trim() } else { stderr.trim() };
        bail!("{prog} failed ({}): {}", out.status, last_lines(detail, 6));
    }
    Ok(stdout)
}

pub fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

fn dbus_error(e: zbus::Error) -> anyhow::Error {
    match &e {
        zbus::Error::MethodError(name, detail, _) => {
            let name = name.as_str();
            let detail = detail.clone().unwrap_or_default();
            if name.ends_with("InteractiveAuthorizationRequired") || name.ends_with("AccessDenied") || name.ends_with("NotAuthorized") {
                anyhow!("not authorized: {detail}")
            } else {
                anyhow!("{detail} ({name})")
            }
        }
        _ => anyhow!(e),
    }
}
