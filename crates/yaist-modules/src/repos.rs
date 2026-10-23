//! Repositories, including community repositories from the Open Build Service.

use anyhow::Result;
use yaist_core::{validate, ActionSpec, Context, ModuleSpec, Output, Param, Registry, Risk, Table};

use crate::zypper;

const ALIAS_PARAM: Param = Param::text("alias", "Repository alias, as listed").check(validate::name);

const LIST: ActionSpec = ActionSpec {
    id: "repos.list",
    title: "List repositories",
    description: "List configured package repositories with their URL, priority and whether they are enabled.",
    risk: Risk::Read,
    params: &[],
};

const ADD: ActionSpec = ActionSpec {
    id: "repos.add",
    title: "Add repository",
    description: "Add a package repository by URL and trust its signing key. Only for sources the user trusts.",
    risk: Risk::Dangerous,
    params: &[
        Param::text("url", "Repository URL (https://…)").check(validate::repo_url),
        Param::text("alias", "Short unique name, e.g. games").check(validate::name),
    ],
};

const ADD_OBS: ActionSpec = ActionSpec {
    id: "repos.add_obs",
    title: "Add OBS project",
    description: "Add a community repository from the Open Build Service (e.g. home:user or devel:languages:rust), built for this distribution, and trust its signing key.",
    risk: Risk::Dangerous,
    params: &[
        Param::text("project", "OBS project, e.g. games or devel:languages:rust").check(validate::obs_project),
        Param::text("alias", "Short unique name; defaults to the project name").optional().check(validate::name),
    ],
};

const ENABLE: ActionSpec =
    ActionSpec { id: "repos.enable", title: "Enable", description: "Enable a repository.", risk: Risk::Change, params: &[ALIAS_PARAM] };
const DISABLE: ActionSpec = ActionSpec {
    id: "repos.disable",
    title: "Disable",
    description: "Disable a repository without removing it.",
    risk: Risk::Change,
    params: &[ALIAS_PARAM],
};
const REMOVE: ActionSpec =
    ActionSpec { id: "repos.remove", title: "Remove", description: "Remove a repository.", risk: Risk::Dangerous, params: &[ALIAS_PARAM] };

pub fn register(reg: &mut Registry) {
    reg.module(ModuleSpec {
        id: "repos",
        title: "Repositories",
        tagline: "Official and community sources",
        overview: LIST.id,
        row_actions: &[ENABLE.id, DISABLE.id, REMOVE.id],
    });

    reg.action(LIST, |ctx, _| async move { list(&ctx).await });

    reg.action(ADD, |ctx, args| async move { add(&ctx, args.str("url"), args.str("alias")).await });

    reg.action(ADD_OBS, |ctx, args| async move {
        let project = args.str("project");
        let url = format!("https://download.opensuse.org/repositories/{}/{}/", project.replace(':', ":/"), ctx.distro.obs_repo_name());
        let alias = args.opt_str("alias").map(str::to_owned).unwrap_or_else(|| project.replace(':', "_"));
        add(&ctx, &url, &alias).await
    });

    reg.action(ENABLE, |ctx, args| async move {
        zypper::change(&ctx, &["modifyrepo", "--enable", "--", args.str("alias")]).await?;
        Ok(Output::Done(format!("Enabled {}", args.str("alias"))))
    });
    reg.action(DISABLE, |ctx, args| async move {
        zypper::change(&ctx, &["modifyrepo", "--disable", "--", args.str("alias")]).await?;
        Ok(Output::Done(format!("Disabled {}", args.str("alias"))))
    });
    reg.action(REMOVE, |ctx, args| async move {
        zypper::change(&ctx, &["removerepo", "--", args.str("alias")]).await?;
        Ok(Output::Done(format!("Removed {}", args.str("alias"))))
    });
}

/// Adds the repository, then refreshes it once to import its key. A
/// repository that can't be refreshed (wrong URL, no build for this
/// distribution) is removed again rather than left half-configured.
async fn add(ctx: &Context, url: &str, alias: &str) -> Result<Output> {
    zypper::change(ctx, &["addrepo", "--refresh", "--", url, alias]).await?;
    if let Err(e) = zypper::change(ctx, &["--gpg-auto-import-keys", "refresh", "--repo", alias]).await {
        let _ = zypper::change(ctx, &["removerepo", "--", alias]).await;
        anyhow::bail!("{url} could not be refreshed, so it was not added: {e}");
    }
    Ok(Output::Done(format!("Added repository {alias} ({url})")))
}

async fn list(ctx: &Context) -> Result<Output> {
    let xml = zypper::query(ctx, &["repos"]).await?;
    let doc = zypper::parse(&xml)?;
    let mut t = Table::new(&[("alias", "Alias"), ("name", "Name"), ("enabled", "Enabled"), ("priority", "Priority"), ("url", "URL")]);
    for r in doc.descendants().filter(|n| n.has_tag_name("repo")) {
        let url = r.children().find(|c| c.has_tag_name("url")).and_then(|u| u.text()).unwrap_or("");
        let attr = |k| r.attribute(k).unwrap_or("").to_owned();
        let enabled = if r.attribute("enabled") == Some("1") { "yes" } else { "no" };
        t.push(vec![attr("alias"), attr("name"), enabled.into(), attr("priority"), url.to_owned()]);
    }
    Ok(Output::Table(t))
}
