//! Distribution upgrades. Tumbleweed and Slowroll roll: an upgrade is a full
//! `zypper dup`. Leap moves to a newer release the way opensuse-migration-tool
//! does (see yaist-helper's migrate-leap).

use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde_json::Value;
use yaist_core::shell_quote as sq;
use yaist_core::{validate, ActionSpec, Context, ModuleSpec, Output, Param, Registry, Risk, Table};

use crate::{helper, zypper};

const RELEASES_API: &str = "https://get.opensuse.org/api/v0/distributions.json";

const TARGETS: ActionSpec = ActionSpec {
    id: "upgrade.targets",
    title: "Upgrade options",
    description: "Show what this system can upgrade to: newer Leap releases, or a full distribution upgrade on Tumbleweed/Slowroll, and which repositories an upgrade would disable.",
    risk: Risk::Read,
    params: &[],
};

const DISTRIBUTION: ActionSpec = ActionSpec {
    id: "upgrade.distribution",
    title: "Upgrade",
    description: "Upgrade the distribution. Tumbleweed/Slowroll: full zypper dup (no version needed). Leap: move to the newer Leap release given as version, switching the openSUSE repositories to it first and disabling third-party repositories pinned to the old release. Takes long; reboot afterwards.",
    risk: Risk::Dangerous,
    params: &[
        Param::text("version", "Target Leap release, e.g. 16.1 (Leap only)").optional(),
        Param::boolean("disable_third_party", "Disable third-party repositories pinned to the current release").default("true"),
    ],
};

pub fn register(reg: &mut Registry) {
    reg.module(ModuleSpec {
        id: "upgrade",
        title: "Upgrade",
        tagline: "New releases and full upgrades",
        overview: TARGETS.id,
        row_actions: &[DISTRIBUTION.id],
    });
    reg.action(TARGETS, |ctx, _| async move { targets(&ctx).await });
    reg.action(DISTRIBUTION, |ctx, args| async move { upgrade(&ctx, args.opt_str("version"), args.bool("disable_third_party")).await });

    reg.shell(TARGETS.id, |_, _| vec!["cat /etc/os-release".into(), "zypper repos --uri".into()]);
    reg.shell(DISTRIBUTION.id, |a, d| {
        if d.rolling {
            return if d.transactional {
                vec!["sudo transactional-update dup".into(), "sudo reboot".into()]
            } else {
                vec!["sudo zypper dup".into(), "sudo reboot".into()]
            };
        }
        let v = sq(a.opt_str("version").unwrap_or("<version>"));
        let mut cmds = Vec::new();
        if a.bool("disable_third_party") {
            cmds.push(format!("sudo zypper modifyrepo --disable <third-party repos pinned to {}>", d.version));
        }
        cmds.extend([
            "sudo zypper install --force-resolution openSUSE-repos-Leap".into(),
            "sudo zypper refresh-services".into(),
            format!("sudo zypper addrepo --refresh https://download.opensuse.org/distribution/leap/{v}/repo/oss tmp"),
            "sudo zypper install --repo tmp --from tmp --force-resolution openSUSE-repos-Leap".into(),
            "sudo zypper removerepo tmp".into(),
            "sudo zypper refresh-services".into(),
            format!("sudo zypper --releasever {v} dup --force-resolution --allow-vendor-change --download-in-advance"),
            "sudo reboot".into(),
        ]);
        cmds
    });
}

fn is_leap(ctx: &Context) -> bool {
    ctx.distro.id == "opensuse-leap"
}

/// Repositories an upgrade may affect: (alias, url, pinned to this release).
async fn third_party(ctx: &Context) -> Result<Vec<(String, String, bool)>> {
    let xml = zypper::query(ctx, &["repos"]).await?;
    let doc = zypper::parse(&xml)?;
    let official = |alias: &str, url: &str| {
        alias.starts_with("openSUSE:")
            || alias.starts_with("NVIDIA:")
            || ["download.opensuse.org/distribution/", "download.opensuse.org/update/", "cdn.opensuse.org/"].iter().any(|p| url.contains(p))
    };
    let version = &ctx.distro.version;
    Ok(doc
        .descendants()
        .filter(|n| n.has_tag_name("repo") && n.attribute("enabled") == Some("1"))
        .filter_map(|r| {
            let alias = r.attribute("alias")?.to_owned();
            let url = r.children().find(|c| c.has_tag_name("url")).and_then(|u| u.text()).unwrap_or("").to_owned();
            (!official(&alias, &url)).then(|| {
                let pinned = !version.is_empty() && url.contains(version.as_str());
                (alias, url, pinned)
            })
        })
        .collect())
}

/// Newer, supported Leap releases from get.opensuse.org: (version, state).
async fn leap_releases(current: &str) -> Result<Vec<(String, String)>> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build()?;
    let v: Value = client.get(RELEASES_API).send().await?.error_for_status()?.json().await.map_err(|e| anyhow!("reading {RELEASES_API}: {e}"))?;
    let mut out: Vec<(String, String)> = v["Leap"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| Some((r["version"].as_str()?.to_owned(), r["state"].as_str().unwrap_or("").to_owned())))
        .filter(|(ver, state)| state != "EOL" && validate::release_version(ver).is_ok() && validate::version_newer(ver, current))
        .collect();
    out.sort_by(|a, b| if validate::version_newer(&a.0, &b.0) { std::cmp::Ordering::Greater } else { std::cmp::Ordering::Less });
    Ok(out)
}

fn x86_64_v2() -> bool {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let flags: Vec<&str> = cpuinfo.lines().find(|l| l.starts_with("flags")).map(|l| l.split_whitespace().collect()).unwrap_or_default();
    ["cx16", "sse4_1", "sse4_2", "popcnt"].iter().all(|f| flags.contains(f))
}

async fn targets(ctx: &Context) -> Result<Output> {
    let d = &ctx.distro;
    let mut t = Table::new(&[("version", "Target"), ("state", "State"), ("notes", "What happens")]);
    if d.rolling {
        let how = if d.transactional {
            "transactional-update dup into a new snapshot; reboot to use it"
        } else {
            "full distribution upgrade with zypper dup"
        };
        t.push(vec!["rolling".into(), "rolling".into(), how.into()]);
        return Ok(Output::Table(t));
    }
    if !is_leap(ctx) {
        return Ok(Output::Text(format!("Upgrades from {} are not supported yet.", if d.pretty.is_empty() { &d.id } else { &d.pretty })));
    }
    let releases = leap_releases(&d.version).await?;
    if releases.is_empty() {
        return Ok(Output::Text(format!("{} is the newest openSUSE Leap release.", d.pretty)));
    }
    let repos = third_party(ctx).await?;
    let pinned: Vec<&str> = repos.iter().filter(|r| r.2).map(|r| r.0.as_str()).collect();
    let kept: Vec<&str> = repos.iter().filter(|r| !r.2).map(|r| r.0.as_str()).collect();
    for (ver, state) in releases {
        let mut notes = vec![format!("switch openSUSE repos to {ver}, then zypper --releasever {ver} dup")];
        if !pinned.is_empty() {
            notes.push(format!("disables {}", pinned.join(", ")));
        }
        if !kept.is_empty() {
            notes.push(format!("keeps {}", kept.join(", ")));
        }
        let major: u32 = ver.split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0);
        if std::env::consts::ARCH == "x86_64" && major >= 16 && !x86_64_v2() {
            notes.insert(0, "NOT POSSIBLE: needs an x86-64-v2 CPU".into());
        }
        t.push(vec![ver, state.to_lowercase(), notes.join("; ")]);
    }
    Ok(Output::Table(t))
}

async fn upgrade(ctx: &Context, version: Option<&str>, disable_third_party: bool) -> Result<Output> {
    let d = &ctx.distro;
    if d.rolling {
        if d.transactional {
            zypper::transactional(ctx, &["dup"]).await?;
        } else {
            zypper::change(ctx, &["dup"]).await?;
        }
        return Ok(Output::Done(format!("{} is fully upgraded; reboot if the kernel or core libraries changed", d.pretty)));
    }
    if !is_leap(ctx) {
        bail!("upgrades from {} are not supported yet", d.id);
    }
    let Some(version) = version.filter(|v| *v != "rolling") else {
        bail!("which Leap release? see upgrade.targets");
    };
    validate::release_version(version).map_err(|e| anyhow!(e))?;
    if !leap_releases(&d.version).await?.iter().any(|(v, _)| v == version) {
        bail!("Leap {version} is not an available upgrade from {}", d.version);
    }
    let aliases: Vec<String> =
        if disable_third_party { third_party(ctx).await?.into_iter().filter(|r| r.2).map(|r| r.0).collect() } else { Vec::new() };
    let mut args = vec!["migrate-leap", version];
    args.extend(aliases.iter().map(String::as_str));
    helper::run(ctx, &args).await?;
    Ok(Output::Done(format!("Upgraded to openSUSE Leap {version}; reboot to finish")))
}
