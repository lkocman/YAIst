//! Talking to zypper: queries run as the user with `--xmlout` against cached
//! metadata; changes run as root (or through transactional-update on
//! read-only-root systems).
//!
//! Later this moves to a libzypp binding (see PLAN.md).

use anyhow::{anyhow, Result};
use yaist_core::{last_lines, Context};

/// Exit codes zypper uses for "succeeded, with something to report":
/// updates/patches available (100-103), nothing found (104), repos skipped (106).
const OK: &[i32] = &[0, 100, 101, 102, 103, 104, 106];

/// Runs a read-only zypper command and returns its XML.
pub async fn query(ctx: &Context, args: &[&str]) -> Result<String> {
    let mut argv = vec!["/usr/bin/zypper", "--xmlout", "--no-refresh", "--non-interactive"];
    argv.extend_from_slice(args);
    ctx.run_ok(&argv, OK).await
}

/// Runs a zypper command as root and returns the tail of its output. Also on
/// transactional systems for repository changes: /etc is writable there (an
/// overlay), only package changes need a transaction.
pub async fn change(ctx: &Context, args: &[&str]) -> Result<String> {
    let mut argv = vec!["/usr/bin/zypper", "--non-interactive"];
    argv.extend_from_slice(args);
    let out = ctx.run_privileged_ok(&argv, OK).await?;
    Ok(last_lines(out.trim(), 3))
}

/// Changes on transactional systems go into a new snapshot that becomes
/// active after a reboot. `--continue` builds on the snapshot that boots next,
/// so several changes before a reboot all land; without it each call would
/// start again from the running system and drop the previous one.
pub async fn transactional(ctx: &Context, args: &[&str]) -> Result<String> {
    let mut argv = vec!["/usr/sbin/transactional-update", "--non-interactive", "--continue"];
    argv.extend_from_slice(args);
    let out = ctx.run_privileged(&argv).await?;
    Ok(format!("{}\nReboot to switch to the new snapshot.", last_lines(out.trim(), 3)))
}

/// Whether a reboot is needed to finish installed changes: a pending
/// transactional snapshot, or updated core libraries/kernel elsewhere.
pub async fn reboot_needed(ctx: &Context) -> bool {
    if std::path::Path::new("/run/reboot-needed").exists() {
        return true;
    }
    !ctx.distro.transactional && ctx.exit_code(&["/usr/bin/zypper", "needs-rebooting"]).await.ok() == Some(102)
}

/// openSUSE's non-oss repository (freely distributable, not open source:
/// Steam, some firmware and fonts): its alias and whether it is enabled.
/// Debug and source variants don't count.
pub async fn non_oss(ctx: &Context) -> Result<Option<(String, bool)>> {
    let xml = query(ctx, &["repos"]).await?;
    let doc = parse(&xml)?;
    let found = doc
        .descendants()
        .filter(|n| n.has_tag_name("repo"))
        .filter_map(|r| {
            let alias = r.attribute("alias")?.to_owned();
            let url = r.children().find(|c| c.has_tag_name("url")).and_then(|u| u.text()).unwrap_or("");
            let is_non_oss = alias.contains("non-oss") || url.contains("/repo/non-oss");
            let variant = alias.contains("debug") || alias.contains("source") || url.contains("/debug/") || url.contains("/source/");
            (is_non_oss && !variant).then(|| (alias, r.attribute("enabled") == Some("1")))
        })
        .max_by_key(|(_, enabled)| *enabled);
    Ok(found)
}

/// The official non-oss URL for this distribution, with zypper's variables so
/// it keeps working after upgrades.
pub fn non_oss_url(d: &yaist_core::Distro) -> String {
    if d.id.contains("slowroll") || d.pretty.contains("Slowroll") {
        "http://cdn.opensuse.org/slowroll/repo/non-oss/".into()
    } else if d.rolling {
        "http://cdn.opensuse.org/tumbleweed/repo/non-oss/".into()
    } else {
        "http://cdn.opensuse.org/distribution/leap/${releasever}/repo/non-oss/$basearch".into()
    }
}

/// Makes sure non-oss is configured and enabled; returns what was done.
pub async fn ensure_non_oss(ctx: &Context) -> Result<Option<String>> {
    match non_oss(ctx).await? {
        Some((_, true)) => Ok(None),
        Some((alias, false)) => {
            change(ctx, &["modifyrepo", "--enable", "--", &alias]).await?;
            Ok(Some(format!("enabled {alias}")))
        }
        None => {
            let url = non_oss_url(&ctx.distro);
            change(ctx, &["addrepo", "--refresh", "--", &url, "repo-non-oss"]).await?;
            change(ctx, &["refresh", "--repo", "repo-non-oss"]).await?;
            Ok(Some("added repo-non-oss".into()))
        }
    }
}

pub fn parse(xml: &str) -> Result<roxmltree::Document<'_>> {
    roxmltree::Document::parse(xml).map_err(|e| anyhow!("could not read zypper's output: {e}"))
}

/// zypper's `<message type="info">` texts, in order.
pub fn info_messages(doc: &roxmltree::Document<'_>) -> Vec<String> {
    doc.descendants()
        .filter(|n| n.has_tag_name("message") && n.attribute("type") == Some("info"))
        .filter_map(|n| n.text().map(str::to_owned))
        .collect()
}
