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

/// Runs a zypper command as root and returns the tail of its output.
pub async fn change(ctx: &Context, args: &[&str]) -> Result<String> {
    let mut argv = vec!["/usr/bin/zypper", "--non-interactive"];
    argv.extend_from_slice(args);
    let out = ctx.run_privileged_ok(&argv, OK).await?;
    Ok(last_lines(out.trim(), 3))
}

/// Package operations on transactional systems go into a new snapshot that
/// becomes active after a reboot.
pub async fn transactional(ctx: &Context, args: &[&str]) -> Result<String> {
    let mut argv = vec!["/usr/sbin/transactional-update", "--non-interactive"];
    argv.extend_from_slice(args);
    let out = ctx.run_privileged(&argv).await?;
    Ok(format!("{}\nReboot to switch to the new snapshot.", last_lines(out.trim(), 3)))
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
