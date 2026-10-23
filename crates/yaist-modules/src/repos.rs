//! Repositories: RPM (zypper, including community repositories from the Open
//! Build Service) and Flatpak remotes (per user, like Flathub).

use anyhow::Result;
use yaist_core::shell_quote as sq;
use yaist_core::{validate, ActionSpec, Context, ModuleSpec, Output, Param, Registry, Risk, Table};

use crate::{flatpak, zypper};

const ALIAS_PARAM: Param = Param::text("alias", "Repository alias, as listed").check(validate::name);
const KIND_PARAM: Param =
    Param::choice("kind", &["rpm", "flatpak-user", "flatpak-system"], "Repository type, as listed (rpm or flatpak)").default("rpm");

const LIST: ActionSpec = ActionSpec {
    id: "repos.list",
    title: "List repositories",
    description: "List configured repositories: RPM repositories (zypper) and Flatpak remotes, with their URL and whether they are enabled.",
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

const ADD_FLATHUB: ActionSpec = ActionSpec {
    id: "repos.add_flathub",
    title: "Add Flathub",
    description: "Add Flathub, the main source of Flatpak apps, for the current user (flatpak --user, no root needed).",
    risk: Risk::Change,
    params: &[],
};

const NON_OSS: ActionSpec = ActionSpec {
    id: "repos.enable_non_oss",
    title: "Enable non-oss",
    description: "Make sure openSUSE's non-oss repository is set up and enabled: freely distributable software that is not open source, such as Steam, some firmware and fonts.",
    risk: Risk::Change,
    params: &[],
};

const ENABLE: ActionSpec = ActionSpec {
    id: "repos.enable",
    title: "Enable",
    description: "Enable a repository (RPM or Flatpak).",
    risk: Risk::Change,
    params: &[ALIAS_PARAM, KIND_PARAM],
};
const DISABLE: ActionSpec = ActionSpec {
    id: "repos.disable",
    title: "Disable",
    description: "Disable a repository (RPM or Flatpak) without removing it.",
    risk: Risk::Change,
    params: &[ALIAS_PARAM, KIND_PARAM],
};
const REMOVE: ActionSpec = ActionSpec {
    id: "repos.remove",
    title: "Remove",
    description: "Remove a repository (RPM or Flatpak).",
    risk: Risk::Dangerous,
    params: &[ALIAS_PARAM, KIND_PARAM],
};

pub fn register(reg: &mut Registry) {
    reg.module(ModuleSpec {
        id: "repos",
        title: "Repositories",
        tagline: "RPM and Flatpak repositories",
        overview: LIST.id,
        row_actions: &[ENABLE.id, DISABLE.id, REMOVE.id],
    });

    reg.action(LIST, |ctx, _| async move { list(&ctx).await });

    reg.action(ADD, |ctx, args| async move { add(&ctx, args.str("url"), args.str("alias")).await });

    reg.action(ADD_OBS, |ctx, args| async move {
        let project = args.str("project");
        let url = obs_url(project, &ctx.distro);
        let alias = args.opt_str("alias").map(str::to_owned).unwrap_or_else(|| project.replace(':', "_"));
        add(&ctx, &url, &alias).await
    });

    reg.action(NON_OSS, |ctx, _| async move {
        Ok(Output::Done(match zypper::ensure_non_oss(&ctx).await? {
            Some(what) => format!("non-oss is ready ({what})"),
            None => "non-oss was already enabled".into(),
        }))
    });
    reg.shell(NON_OSS.id, |_, d| {
        vec![
            "sudo zypper modifyrepo --enable openSUSE:repo-non-oss   # if it exists, disabled".into(),
            format!("sudo zypper addrepo --refresh '{}' repo-non-oss   # if it is missing", zypper::non_oss_url(d)),
        ]
    });

    reg.action(ADD_FLATHUB, |ctx, _| async move {
        flatpak::ensure_flathub(&ctx).await?;
        Ok(Output::Done("Flathub added for your user".into()))
    });

    for (spec, verb) in [(ENABLE, "Enabled"), (DISABLE, "Disabled"), (REMOVE, "Removed")] {
        reg.action(spec, move |ctx, args| async move {
            let alias = args.str("alias");
            match args.str("kind") {
                "rpm" => {
                    let op: &[&str] = match spec.verb() {
                        "enable" => &["modifyrepo", "--enable"],
                        "disable" => &["modifyrepo", "--disable"],
                        _ => &["removerepo"],
                    };
                    let mut zargs = op.to_vec();
                    zargs.extend(["--", alias]);
                    zypper::change(&ctx, &zargs).await?;
                }
                kind => {
                    let user = kind == "flatpak-user";
                    match spec.verb() {
                        "enable" => flatpak::remote_cmd(&ctx, user, &["remote-modify", "--enable", alias]).await?,
                        "disable" => flatpak::remote_cmd(&ctx, user, &["remote-modify", "--disable", alias]).await?,
                        _ => flatpak::remote_cmd(&ctx, user, &["remote-delete", alias]).await?,
                    }
                }
            }
            Ok(Output::Done(format!("{verb} {alias}")))
        });
    }

    reg.shell(LIST.id, |_, _| vec!["zypper repos --uri".into()]);
    reg.shell(ADD.id, |a, _| add_cmds(a.str("url"), a.str("alias")));
    reg.shell(ADD_OBS.id, |a, d| {
        let project = a.str("project");
        let alias = a.opt_str("alias").map(str::to_owned).unwrap_or_else(|| project.replace(':', "_"));
        add_cmds(&obs_url(project, d), &alias)
    });
    reg.shell(ADD_FLATHUB.id, |_, _| vec![format!("flatpak remote-add --user --if-not-exists {} {}", flatpak::FLATHUB, flatpak::FLATHUB_URL)]);
    reg.shell(ENABLE.id, |a, _| repo_cmd(a, "sudo zypper modifyrepo --enable", "flatpak remote-modify --enable"));
    reg.shell(DISABLE.id, |a, _| repo_cmd(a, "sudo zypper modifyrepo --disable", "flatpak remote-modify --disable"));
    reg.shell(REMOVE.id, |a, _| repo_cmd(a, "sudo zypper removerepo", "flatpak remote-delete"));
}

fn repo_cmd(a: &yaist_core::Args, rpm: &str, flatpak: &str) -> Vec<String> {
    let alias = sq(a.str("alias"));
    match a.str("kind") {
        "flatpak-user" => vec![format!("{flatpak} --user {alias}")],
        "flatpak-system" => vec![format!("{flatpak} --system {alias}")],
        _ => vec![format!("{rpm} {alias}")],
    }
}

/// The OBS repository URL for `project` on this distribution.
fn obs_url(project: &str, d: &yaist_core::Distro) -> String {
    format!("https://download.opensuse.org/repositories/{}/{}/", project.replace(':', ":/"), d.obs_repo_name())
}

fn add_cmds(url: &str, alias: &str) -> Vec<String> {
    vec![
        format!("sudo zypper addrepo --refresh {} {}", sq(url), sq(alias)),
        format!("sudo zypper --gpg-auto-import-keys refresh --repo {}", sq(alias)),
    ]
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
    let mut t =
        Table::new(&[("alias", "Alias"), ("name", "Name"), ("kind", "Type"), ("enabled", "Enabled"), ("priority", "Priority"), ("url", "URL")]);
    for r in doc.descendants().filter(|n| n.has_tag_name("repo")) {
        let url = r.children().find(|c| c.has_tag_name("url")).and_then(|u| u.text()).unwrap_or("");
        let attr = |k| r.attribute(k).unwrap_or("").to_owned();
        let enabled = if r.attribute("enabled") == Some("1") { "yes" } else { "no" };
        t.push(vec![attr("alias"), attr("name"), "rpm".into(), enabled.into(), attr("priority"), url.to_owned()]);
    }
    for remote in flatpak::remotes(ctx).await {
        let kind = if remote.user { "flatpak-user" } else { "flatpak-system" };
        t.push(vec![remote.name, remote.title, kind.into(), if remote.enabled { "yes" } else { "no" }.into(), "-".into(), remote.url]);
    }
    Ok(Output::Table(t))
}
