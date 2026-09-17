//! zbus proxies for `org.modulix.Daemon`'s two interfaces.
//!
//! `#[zbus::proxy]` (with the `blocking-api` feature, on by default in zbus'
//! own default feature set) generates both an async `StoreProxy`/`DaemonProxy`
//! for Rust consumers and a blocking `StoreProxyBlocking`/`DaemonProxyBlocking`
//! companion for [`crate::ffi`], which runs in the caller's own process
//! (GNOME Software) with no Tokio runtime of its own.

use std::collections::HashMap;

use zbus::zvariant::OwnedValue;

/// One `a{sv}` entry: an [`crate::AppEntry`]/plugin/enrichment row as it
/// crosses the bus. See `modulix-daemon/src/store/entry.rs` for the field
/// list emitted by the server side.
pub type Dict = HashMap<String, OwnedValue>;

/// `org.modulix.Store1`: reads (search, listings, enrichment, alternate
/// sources). Unprivileged — reachable by any caller on the system bus.
#[zbus::proxy(
    interface = "org.modulix.Store1",
    default_service = "org.modulix.Daemon",
    default_path = "/org/modulix/Daemon"
)]
pub trait Store {
    /// Whether the daemon's on-disk nix package index is currently servable.
    #[zbus(property)]
    fn index_ready(&self) -> zbus::Result<bool>;

    fn search_packages(&self, query: &str, max: u32) -> zbus::Result<Vec<Dict>>;
    fn search_modules(&self, query: &str, max: u32) -> zbus::Result<Vec<Dict>>;
    fn list_installed_packages(&self) -> zbus::Result<Vec<Dict>>;
    fn list_installed_modules(&self) -> zbus::Result<Vec<Dict>>;
    fn list_module_plugins(&self, module: &str) -> zbus::Result<Vec<Dict>>;
    fn get_app_enrichment(&self, app_ids: Vec<&str>) -> zbus::Result<HashMap<String, Dict>>;
    fn packages_for_app_id(&self, app_id: &str) -> zbus::Result<Vec<Dict>>;
    fn get_package_licenses(&self, attrs: Vec<&str>) -> zbus::Result<HashMap<String, String>>;
}

/// `org.modulix.Daemon`: writes (install/uninstall). Reaching the method is
/// unrestricted at the bus-policy level; polkit gates it per-call.
#[zbus::proxy(
    interface = "org.modulix.Daemon",
    default_service = "org.modulix.Daemon",
    default_path = "/org/modulix/Daemon"
)]
pub trait Daemon {
    fn install_package(&self, names: Vec<&str>) -> zbus::Result<String>;
    fn uninstall_package(&self, names: Vec<&str>) -> zbus::Result<String>;
    fn install_module(&self, names: Vec<&str>) -> zbus::Result<String>;
    fn uninstall_module(&self, names: Vec<&str>) -> zbus::Result<String>;
    fn install_plugin(&self, module: &str, plugin: &str) -> zbus::Result<String>;
    fn uninstall_plugin(&self, module: &str, plugin: &str) -> zbus::Result<String>;
}
