//! Packages: search, details, install/remove and system updates. The update
//! command follows the distribution: `zypper up` on Leap, `zypper dup` on
//! rolling releases, `transactional-update` on immutable variants.

use anyhow::Result;
use yaist_core::{validate, ActionSpec, Context, ModuleSpec, Output, Param, Registry, Risk, Table};

use crate::zypper;

const PACKAGES_PARAM: Param = Param::text("packages", "Package names, space separated").check(validate::packages);

const UPDATES: ActionSpec = ActionSpec {
    id: "packages.updates",
    title: "Available updates",
    description: "List packages with updates available, from the cached repository metadata.",
    risk: Risk::Read,
    params: &[],
};

const SEARCH: ActionSpec = ActionSpec {
    id: "packages.search",
    title: "Search",
    description: "Search packages by name in the enabled repositories, showing whether each is installed.",
    risk: Risk::Read,
    params: &[Param::text("query", "Search term, e.g. nginx or *vim*").check(validate::search_term)],
};

const INFO: ActionSpec = ActionSpec {
    id: "packages.info",
    title: "Details",
    description: "Show a package's version, repository, install status and description.",
    risk: Risk::Read,
    params: &[Param::text("packages", "Package name").check(validate::packages)],
};

const INSTALL: ActionSpec = ActionSpec {
    id: "packages.install",
    title: "Install",
    description: "Install packages (and their dependencies).",
    risk: Risk::Change,
    params: &[PACKAGES_PARAM],
};

const UPDATE: ActionSpec = ActionSpec {
    id: "packages.update",
    title: "Update",
    description: "Update specific packages to their newest version.",
    risk: Risk::Change,
    params: &[PACKAGES_PARAM],
};

const REMOVE: ActionSpec = ActionSpec {
    id: "packages.remove",
    title: "Remove",
    description: "Remove packages. Packages that depend on them are removed too.",
    risk: Risk::Dangerous,
    params: &[PACKAGES_PARAM],
};

const UPDATE_SYSTEM: ActionSpec = ActionSpec {
    id: "packages.update_system",
    title: "Update system",
    description: "Update the whole system the way this distribution expects (Leap: zypper update, Tumbleweed/Slowroll: zypper dup, immutable: transactional-update).",
    risk: Risk::Dangerous,
    params: &[],
};

const REFRESH: ActionSpec = ActionSpec {
    id: "packages.refresh",
    title: "Refresh",
    description: "Download fresh metadata from all enabled repositories.",
    risk: Risk::Change,
    params: &[],
};

pub fn register(reg: &mut Registry) {
    reg.module(ModuleSpec { id: "packages", title: "Packages", tagline: "Install, remove and update", overview: UPDATES.id, row_actions: &[UPDATE.id, INFO.id] });

    reg.action(UPDATES, |ctx, _| async move { updates(&ctx).await });
    reg.action(SEARCH, |ctx, args| async move { search(&ctx, args.str("query")).await });
    reg.action(INFO, |ctx, args| async move { info(&ctx, args.str("packages")).await });

    reg.action(INSTALL, |ctx, args| async move {
        let pkgs: Vec<&str> = args.str("packages").split_whitespace().collect();
        let out = pkg_op(&ctx, "install", &pkgs).await?;
        Ok(Output::Done(format!("Installed {}. {out}", pkgs.join(", "))))
    });
    reg.action(UPDATE, |ctx, args| async move {
        let pkgs: Vec<&str> = args.str("packages").split_whitespace().collect();
        let out = pkg_op(&ctx, "update", &pkgs).await?;
        Ok(Output::Done(format!("Updated {}. {out}", pkgs.join(", "))))
    });
    reg.action(REMOVE, |ctx, args| async move {
        let pkgs: Vec<&str> = args.str("packages").split_whitespace().collect();
        let out = pkg_op(&ctx, "remove", &pkgs).await?;
        Ok(Output::Done(format!("Removed {}. {out}", pkgs.join(", "))))
    });

    reg.action(UPDATE_SYSTEM, |ctx, _| async move {
        let d = &ctx.distro;
        let verb = if d.rolling { "dup" } else { "update" };
        let out = if d.transactional { zypper::transactional(&ctx, &[if d.rolling { "dup" } else { "up" }]).await? } else { zypper::change(&ctx, &[verb]).await? };
        Ok(Output::Done(format!("System updated ({verb}). {out}")))
    });

    reg.action(REFRESH, |ctx, _| async move {
        zypper::change(&ctx, &["refresh"]).await?;
        Ok(Output::Done("Repository metadata refreshed".into()))
    });
}

async fn pkg_op(ctx: &Context, op: &str, pkgs: &[&str]) -> Result<String> {
    let mut args = vec![op, "--"];
    args.extend_from_slice(pkgs);
    if ctx.distro.transactional {
        let mut targs = vec!["pkg"];
        targs.extend(args);
        zypper::transactional(ctx, &targs).await
    } else {
        zypper::change(ctx, &args).await
    }
}

async fn updates(ctx: &Context) -> Result<Output> {
    let xml = zypper::query(ctx, &["list-updates"]).await?;
    let doc = zypper::parse(&xml)?;
    let mut t = Table::new(&[("packages", "Package"), ("current", "Installed"), ("new", "Available"), ("repo", "Repository")]);
    for u in doc.descendants().filter(|n| n.has_tag_name("update")) {
        let repo = u.children().find(|c| c.has_tag_name("source")).and_then(|s| s.attribute("alias")).unwrap_or("");
        let attr = |k| u.attribute(k).unwrap_or("").to_owned();
        t.push(vec![attr("name"), attr("edition-old"), attr("edition"), repo.to_owned()]);
    }
    Ok(Output::Table(t))
}

async fn search(ctx: &Context, query: &str) -> Result<Output> {
    let xml = zypper::query(ctx, &["search", "--", query]).await?;
    let doc = zypper::parse(&xml)?;
    let mut t = Table::new(&[("packages", "Package"), ("status", "Status"), ("summary", "Summary")]);
    for s in doc.descendants().filter(|n| n.has_tag_name("solvable")) {
        if s.attribute("kind").is_some_and(|k| k != "package") {
            continue;
        }
        let status = if s.attribute("status") == Some("installed") { "installed" } else { "available" };
        t.push(vec![s.attribute("name").unwrap_or("").into(), status.into(), s.attribute("summary").unwrap_or("").into()]);
    }
    t.rows.truncate(200);
    Ok(Output::Table(t))
}

/// `zypper info` has no structured output, only "Key : value" lines.
async fn info(ctx: &Context, package: &str) -> Result<Output> {
    let xml = zypper::query(ctx, &["info", "--", package]).await?;
    let doc = zypper::parse(&xml)?;
    let mut facts: Vec<(String, String)> = Vec::new();
    for msg in zypper::info_messages(&doc).iter().filter(|m| m.contains(" : ")) {
        for line in msg.lines() {
            match line.split_once(" : ") {
                Some((k, v)) if !k.starts_with(' ') && !k.trim().is_empty() => facts.push((k.trim().to_owned(), v.trim().to_owned())),
                _ => {
                    if let Some(last) = facts.last_mut() {
                        last.1.push('\n');
                        last.1.push_str(line.trim());
                    }
                }
            }
        }
    }
    if facts.is_empty() {
        return Ok(Output::Text(format!("No package called '{package}' found in the enabled repositories")));
    }
    Ok(Output::Facts(facts))
}
