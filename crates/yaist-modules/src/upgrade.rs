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
    description: "Upgrade the distribution. Tumbleweed/Slowroll: full zypper dup (no version needed). Leap and Leap Micro (also immutable): move to the newer release given as version, switching the openSUSE repositories to it first and disabling every other repository (OBS, Packman, third-party: they often have no build for the new release yet, and one repository that can't be refreshed makes zypper dup fail). Takes long; reboot afterwards.",
    risk: Risk::Dangerous,
    params: &[
        Param::text("version", "Target Leap release, e.g. 16.1 (Leap only)").optional(),
        Param::boolean("disable_third_party", "Disable all repositories that are not openSUSE's own (recommended)").default("true"),
        Param::text(
            "keep_enabled",
            "Repository aliases to keep enabled anyway, space separated. Only release-independent ones: a kept repository that can't be refreshed for the new release makes zypper dup fail. Repositories using $releasever that already have the new release are kept automatically",
        )
        .default("google-chrome")
        .check(validate::names),
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
    reg.action(DISTRIBUTION, |ctx, args| async move {
        let keep: Vec<String> = args.str("keep_enabled").split_whitespace().map(str::to_owned).collect();
        upgrade(&ctx, args.opt_str("version"), args.bool("disable_third_party"), &keep).await
    });

    reg.shell(TARGETS.id, |_, _| vec!["cat /etc/os-release".into(), "zypper repos --uri".into()]);
    reg.shell(DISTRIBUTION.id, |a, d| {
        if d.rolling {
            return if d.transactional {
                vec!["sudo transactional-update --continue dup".into(), "sudo reboot".into()]
            } else {
                vec!["sudo zypper dup".into(), "sudo reboot".into()]
            };
        }
        let v = sq(a.opt_str("version").unwrap_or("<version>"));
        let (repos_pkg, url) = if d.id == "opensuse-leap-micro" {
            (
                "openSUSE-repos-LeapMicro",
                format!("https://download.opensuse.org/distribution/leap-micro/{v}/product/repo/openSUSE-Leap-Micro-{v}-$(uname -m)/"),
            )
        } else {
            ("openSUSE-repos-Leap", format!("https://download.opensuse.org/distribution/leap/{v}/repo/oss"))
        };
        // On transactional systems each step runs inside the new snapshot.
        let z = if d.transactional { "sudo transactional-update --continue run zypper" } else { "sudo zypper" };
        let mut cmds = Vec::new();
        if a.bool("disable_third_party") {
            cmds.push(format!("{z} modifyrepo --disable <every repository except openSUSE:*, NVIDIA:* and {}>", a.str("keep_enabled")));
        }
        cmds.extend([
            format!("{z} install --force-resolution {repos_pkg}"),
            format!("{z} refresh-services"),
            format!("{z} addrepo --refresh {url} tmp"),
            format!("{z} install --repo tmp --from tmp --force-resolution {repos_pkg}"),
            format!("{z} removerepo tmp"),
            format!("{z} refresh-services"),
            format!("{z} --releasever {v} dup --force-resolution --allow-vendor-change --download-in-advance"),
            "sudo reboot".into(),
        ]);
        cmds
    });
}

/// The Leap line this system follows: (get.opensuse.org key, helper flavour,
/// display name). Immutable Leap is `opensuse-leap` too, with
/// `VARIANT_ID=transactional`.
fn leap_line(d: &yaist_core::Distro) -> Option<(&'static str, &'static str, &'static str)> {
    match d.id.as_str() {
        "opensuse-leap" => Some(("Leap", "leap", "openSUSE Leap")),
        "opensuse-leap-micro" => Some(("LeapMicro", "leap-micro", "openSUSE Leap Micro")),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Fate {
    /// openSUSE's own, switched to the new release by openSUSE-repos.
    Official,
    /// Release-independent, asked to be kept.
    Keep,
    /// Follows $releasever; kept only if it already has the new release.
    CheckRelease,
    Disable,
}

/// What happens to an enabled repository in a Leap upgrade, following
/// opensuse-migration-tool: openSUSE's own repositories switch with
/// openSUSE-repos, release-independent ones the user keeps stay, repositories
/// using $releasever stay if they already have the new release (checked
/// online), and everything else is disabled: hard-coded old releases, OBS
/// leftovers that would ask for impossible upgrades, vendor repositories.
fn repo_fate(alias: &str, raw_url: &str, keep: &[String]) -> Fate {
    if alias.starts_with("openSUSE:") || alias.starts_with("NVIDIA:") {
        Fate::Official
    } else if keep.iter().any(|k| k == alias) {
        Fate::Keep
    } else if raw_url.contains("releasever") {
        Fate::CheckRelease
    } else {
        Fate::Disable
    }
}

/// `baseurl` of a repository as written in /etc/zypp/repos.d, variables
/// unexpanded.
fn raw_baseurl(alias: &str) -> String {
    let header = format!("[{alias}]");
    std::fs::read_dir("/etc/zypp/repos.d")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .find_map(|text| {
            let mut in_section = false;
            for line in text.lines() {
                let line = line.trim();
                if line.starts_with('[') {
                    in_section = line == header;
                } else if in_section {
                    if let Some(url) = line.strip_prefix("baseurl=") {
                        return Some(url.trim().to_owned());
                    }
                }
            }
            None
        })
        .unwrap_or_default()
}

/// Does the repository already have metadata for `target`?
async fn has_release(client: &reqwest::Client, raw_url: &str, target: &str) -> bool {
    let arch = std::env::consts::ARCH;
    let url = raw_url.replace("${releasever}", target).replace("$releasever", target).replace("${basearch}", arch).replace("$basearch", arch);
    let probe = format!("{}/repodata/repomd.xml", url.trim_end_matches('/'));
    client.head(&probe).send().await.map(|r| r.status().is_success()).unwrap_or(false)
}

/// Repositories to disable before upgrading to `target`, and those kept with
/// the reason.
async fn repos_to_disable(ctx: &Context, keep: &[String], target: &str) -> Result<(Vec<String>, Vec<String>)> {
    let xml = zypper::query(ctx, &["repos"]).await?;
    let doc = zypper::parse(&xml)?;
    let client = reqwest::Client::builder().timeout(Duration::from_secs(8)).build()?;
    let mut disable = Vec::new();
    let mut kept = Vec::new();
    let mut checks = Vec::new();
    for r in doc.descendants().filter(|n| n.has_tag_name("repo") && n.attribute("enabled") == Some("1")) {
        let Some(alias) = r.attribute("alias") else { continue };
        let raw = raw_baseurl(alias);
        match repo_fate(alias, &raw, keep) {
            Fate::Official => {}
            Fate::Keep => kept.push(format!("{alias} (kept)")),
            Fate::Disable => disable.push(alias.to_owned()),
            Fate::CheckRelease => checks.push((alias.to_owned(), raw)),
        }
    }
    // Probe the $releasever repositories concurrently.
    let tasks: Vec<_> = checks
        .into_iter()
        .map(|(alias, raw)| {
            let (client, target) = (client.clone(), target.to_owned());
            tokio::spawn(async move {
                let ready = has_release(&client, &raw, &target).await;
                (alias, ready)
            })
        })
        .collect();
    for task in tasks {
        let (alias, ready) = task.await?;
        if ready {
            kept.push(format!("{alias} (has {target})"));
        } else {
            disable.push(alias);
        }
    }
    Ok((disable, kept))
}

/// Newer, supported releases of a Leap line from get.opensuse.org: (version, state).
async fn leap_releases(key: &str, current: &str) -> Result<Vec<(String, String)>> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build()?;
    let v: Value = client.get(RELEASES_API).send().await?.error_for_status()?.json().await.map_err(|e| anyhow!("reading {RELEASES_API}: {e}"))?;
    let mut out: Vec<(String, String)> = v[key]
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
    let Some((key, flavour, line)) = leap_line(d) else {
        return Ok(Output::Text(format!("Upgrades from {} are not supported yet.", if d.pretty.is_empty() { &d.id } else { &d.pretty })));
    };
    let releases = leap_releases(key, &d.version).await?;
    if releases.is_empty() {
        return Ok(Output::Text(format!("{} is the newest {line} release.", d.pretty)));
    }
    let keep = ["google-chrome".to_owned()];
    for (ver, state) in releases {
        let mut notes = vec![format!("switch openSUSE repos to {ver}, then zypper --releasever {ver} dup")];
        if d.transactional {
            notes.push("inside a new snapshot (transactional-update run); reboot to use it".into());
        }
        let (disable, kept) = repos_to_disable(ctx, &keep, &ver).await?;
        if !disable.is_empty() {
            notes.push(format!("disables {} other repositories: {}", disable.len(), disable.join(", ")));
        }
        if !kept.is_empty() {
            notes.push(format!("keeps {}", kept.join(", ")));
        }
        let major: u32 = ver.split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0);
        if flavour == "leap" && std::env::consts::ARCH == "x86_64" && major >= 16 && !x86_64_v2() {
            notes.insert(0, "NOT POSSIBLE: needs an x86-64-v2 CPU".into());
        }
        t.push(vec![ver, state.to_lowercase(), notes.join("; ")]);
    }
    Ok(Output::Table(t))
}

async fn upgrade(ctx: &Context, version: Option<&str>, disable_third_party: bool, keep: &[String]) -> Result<Output> {
    let d = &ctx.distro;
    if d.rolling {
        if d.transactional {
            zypper::transactional(ctx, &["dup"]).await?;
        } else {
            zypper::change(ctx, &["dup"]).await?;
        }
        return Ok(Output::Done(format!("{} is fully upgraded; reboot if the kernel or core libraries changed", d.pretty)));
    }
    let Some((key, flavour, _)) = leap_line(d) else {
        bail!("upgrades from {} are not supported yet", d.id);
    };
    let Some(version) = version.filter(|v| *v != "rolling") else {
        bail!("which Leap release? see upgrade.targets");
    };
    validate::release_version(version).map_err(|e| anyhow!(e))?;
    if !leap_releases(key, &d.version).await?.iter().any(|(v, _)| v == version) {
        bail!("{version} is not an available upgrade from {}", d.version);
    }
    let aliases: Vec<String> = if disable_third_party { repos_to_disable(ctx, keep, version).await?.0 } else { Vec::new() };
    let mut args = vec!["migrate-leap", flavour, version];
    args.extend(aliases.iter().map(String::as_str));
    helper::run(ctx, &args).await?;
    let mut msg = format!("Upgraded to {version}; reboot to finish");
    if !aliases.is_empty() {
        msg.push_str(&format!(
            ". Disabled {}: enable them again (Repositories) once they have builds for {version}. Packages they provided may be orphaned now: zypper packages --orphaned",
            aliases.join(", ")
        ));
    }
    Ok(Output::Done(msg))
}

#[cfg(test)]
mod tests {
    use super::{repo_fate, Fate};

    #[test]
    fn repositories_in_an_upgrade() {
        let keep = vec!["google-chrome".to_owned()];
        let fate = |alias: &str, url: &str| repo_fate(alias, url, &keep);
        assert_eq!(fate("openSUSE:repo-oss", "http://cdn.opensuse.org/distribution/leap/${releasever}/repo/oss/$basearch"), Fate::Official);
        assert_eq!(fate("NVIDIA:repo-non-free", "https://download.nvidia.com/opensuse/leap/$releasever"), Fate::Official);
        assert_eq!(fate("google-chrome", "https://dl.google.com/linux/chrome/rpm/stable/x86_64"), Fate::Keep);
        assert_eq!(fate("packman", "https://ftp.fau.de/packman/suse/openSUSE_Leap_$releasever/"), Fate::CheckRelease);
        assert_eq!(fate("home_lkocman_kudos", "https://download.opensuse.org/repositories/home:/lkocman:/kudos/16.0/"), Fate::Disable);
        assert_eq!(fate("old_obs", "https://download.opensuse.org/repositories/games/15.6/"), Fate::Disable);
        assert_eq!(fate("code", "https://packages.microsoft.com/yumrepos/vscode"), Fate::Disable);
    }
}
