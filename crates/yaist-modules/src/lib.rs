//! The system modules. Each registers a [`yaist_core::ModuleSpec`] and its
//! actions; clients never call these directly, only through the registry.

mod firewall;
mod packages;
mod repos;
mod selinux;
mod services;
mod system;
mod users;
mod zypper;

use yaist_core::Registry;

/// All modules, in sidebar order.
pub fn registry() -> Registry {
    let mut reg = Registry::default();
    system::register(&mut reg);
    users::register(&mut reg);
    services::register(&mut reg);
    firewall::register(&mut reg);
    selinux::register(&mut reg);
    packages::register(&mut reg);
    repos::register(&mut reg);
    reg
}
