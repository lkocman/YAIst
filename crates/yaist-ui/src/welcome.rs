//! The welcome screen, shown first: a greeting, starting points, questions to
//! try, the system at a glance and pointers into the openSUSE community. It
//! takes over from openSUSE-welcome. Both clients render this content.

use yaist_core::Distro;

/// How this system takes changes, shown under the welcome title:
/// (label, explanation).
pub fn mode(d: &Distro) -> (&'static str, &'static str) {
    if d.transactional {
        if d.rolling {
            ("Immutable", "Read-only rolling release: updates and software go into a new snapshot, used after a reboot.")
        } else {
            ("Immutable", "Read-only system: updates and software go into a new snapshot, used after a reboot.")
        }
    } else if d.rolling {
        ("Standard", "Rolling release: always the latest software, updated with zypper dup.")
    } else {
        ("Standard", "Regular release: updates and software install directly with zypper.")
    }
}

/// What a starting point does when chosen.
#[derive(Clone, Copy, Debug)]
pub enum Start {
    /// Run (reads) or propose (changes) an action; a form opens if it needs input.
    Action(&'static str),
    /// Open a module's panel.
    Module(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub struct StartItem {
    pub title: &'static str,
    pub text: &'static str,
    pub start: Start,
}

pub fn start_items(d: &Distro) -> Vec<StartItem> {
    let update = if d.transactional {
        StartItem {
            title: "Install updates",
            text: "Updates go into a new snapshot; reboot to use them, or roll back.",
            start: Start::Action("packages.update_system"),
        }
    } else if d.rolling {
        StartItem {
            title: "Update Tumbleweed",
            text: "Bring the system up to the latest snapshot with zypper dup.",
            start: Start::Action("upgrade.distribution"),
        }
    } else {
        StartItem {
            title: "Install updates",
            text: "Get the latest maintenance and security updates.",
            start: Start::Action("packages.update_system"),
        }
    };
    let mut items = vec![
        update,
        StartItem {
            title: "Take a snapshot",
            text: "Save the system state with snapper, so you can roll back.",
            start: Start::Action("snapshots.create"),
        },
        StartItem {
            title: "Find software",
            text: "Search the openSUSE repositories and install with one click.",
            start: Start::Action("packages.search"),
        },
        StartItem { title: "Add a user", text: "Create an account for someone else on this computer.", start: Start::Action("users.create") },
        StartItem { title: "Check the firewall", text: "See which services and ports are open to the network.", start: Start::Module("firewall") },
    ];
    items.push(if d.id == "opensuse-leap" {
        StartItem { title: "New Leap release?", text: "See whether a newer openSUSE Leap is available.", start: Start::Module("upgrade") }
    } else {
        StartItem { title: "Community software", text: "Add repositories from the Open Build Service.", start: Start::Module("repos") }
    });
    items
}

/// Requests to try in the conversation.
pub fn suggestions(d: &Distro) -> Vec<&'static str> {
    let mut s = vec!["Is my system up to date?", "Which services failed?", "Install htop", "Open port 8080 for my dev server"];
    s.push(if d.id == "opensuse-leap" { "Upgrade to the next Leap" } else { "Take a snapshot before I update" });
    s.push("zypper in fish");
    s
}

#[derive(Clone, Copy, Debug)]
pub struct Link {
    pub title: &'static str,
    pub text: &'static str,
    pub url: &'static str,
}

pub const LINKS: &[Link] = &[
    Link { title: "Documentation", text: "Start-up guide, reference and how-tos", url: "https://doc.opensuse.org/" },
    Link { title: "Release notes", text: "What is new and what to watch out for", url: "https://doc.opensuse.org/release-notes/" },
    Link { title: "Wiki", text: "Community knowledge and support database", url: "https://en.opensuse.org/Portal" },
    Link { title: "Forums", text: "Ask questions and help others", url: "https://forums.opensuse.org/" },
    Link { title: "Chat", text: "Talk to the community on Matrix", url: "https://chat.opensuse.org/" },
    Link { title: "Software", text: "Find packages, including community builds", url: "https://software.opensuse.org/" },
    Link { title: "Open Build Service", text: "Build and share your own packages", url: "https://build.opensuse.org/" },
    Link { title: "Report a bug", text: "Bugzilla, where problems get fixed", url: "https://bugzilla.opensuse.org/" },
    Link { title: "News", text: "Project news and announcements", url: "https://news.opensuse.org/" },
];

/// The chameleon for terminals: our own drawing, not the Geeko logo.
pub const ASCII_CHAMELEON: &[&str] = &[
    r"            __.---.__          ",
    r"        _.-'  .  .   '-._      ",
    r"      .'   .   .   .  ( @ )\   ",
    r"     /  .   .   .   .  '-'  >  ",
    r"     \_   .   .   .   _.---'   ",
    r"    (@ '-.__.-.__.-.-'         ",
    r"     '-'  /_/     /_/          ",
    r"   =====================~~     ",
];
