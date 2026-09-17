//! C-ABI shim consumed by `gnome-software-plugin/plugin/src/gs-plugin-modulix.c`.
//!
//! Every read crosses as a JSON string (see [`crate::convert`]) — the plugin's
//! existing json-glib parsing is unchanged from when this was
//! `gnome-software-plugin/backend`; only the transport moved from in-process
//! FFI into `modulix-core-utils` to a D-Bus round-trip to `mx-daemon`. Writes
//! go straight to `org.modulix.Daemon` (polkit-gated on the daemon side) —
//! the plugin's own `daemon_call()` GDBus helper is gone.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_uint};
use std::ptr;
use std::sync::OnceLock;

use serde_json::Value as Json;
use zbus::blocking::Connection;

use crate::convert::dict_to_json;
use crate::proxy::{DaemonProxyBlocking, StoreProxyBlocking};

pub type StoreResult = i32;
pub const STORE_OK: StoreResult = 0;
pub const STORE_ERROR: StoreResult = 1;

struct Clients {
    store: StoreProxyBlocking<'static>,
    daemon: DaemonProxyBlocking<'static>,
}

static CLIENTS: OnceLock<Option<Clients>> = OnceLock::new();

fn clients() -> Option<&'static Clients> {
    CLIENTS
        .get_or_init(|| {
            let connection = Connection::system()
                .inspect_err(|e| eprintln!("[store-client] system bus connect: {e}"))
                .ok()?;
            let store = StoreProxyBlocking::new(&connection)
                .inspect_err(|e| eprintln!("[store-client] Store1 proxy: {e}"))
                .ok()?;
            let daemon = DaemonProxyBlocking::new(&connection)
                .inspect_err(|e| eprintln!("[store-client] Daemon proxy: {e}"))
                .ok()?;
            Some(Clients { store, daemon })
        })
        .as_ref()
}

/// Establishes the system-bus connection and proxies used by every other
/// `mx_store_*` call. Safe to call more than once (idempotent).
#[unsafe(no_mangle)]
pub extern "C" fn mx_store_init() -> StoreResult {
    if clients().is_some() {
        STORE_OK
    } else {
        STORE_ERROR
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn mx_store_shutdown() {
    // The connection lives for the whole process; nothing to tear down.
}

// ─── Memory / string helpers ────────────────────────────────────────────────

fn to_cstring(s: String) -> *mut c_char {
    CString::new(s)
        .map(CString::into_raw)
        .unwrap_or(ptr::null_mut())
}

fn cstr_to_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: caller guarantees `ptr` is a valid NUL-terminated C string.
    unsafe { CStr::from_ptr(ptr).to_str().ok() }
}

/// Collects up to `n` NUL-terminated C strings from `ptr[0..n)` into owned
/// `String`s, skipping any NULL entries.
///
/// # Safety
/// `ptr` must be NULL, or point to at least `n` valid `*const c_char`
/// entries, each NULL or a valid NUL-terminated string.
unsafe fn collect_strv(ptr: *const *const c_char, n: c_uint) -> Vec<String> {
    if ptr.is_null() {
        return Vec::new();
    }
    (0..n as isize)
        // SAFETY: caller guarantees `ptr[0..n)` are valid.
        .filter_map(|i| cstr_to_str(unsafe { *ptr.offset(i) }).map(str::to_string))
        .collect()
}

/// Free a string previously returned by any `mx_store_*` function.
///
/// # Safety
/// `ptr` must be either NULL or a pointer obtained from a `mx_store_*`
/// function in this library and not yet freed. Passing any other pointer, or
/// freeing the same pointer twice, is undefined behaviour.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mx_store_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        // SAFETY: `ptr` was produced by `CString::into_raw` in this module.
        unsafe { drop(CString::from_raw(ptr)) };
    }
}

fn dicts_json(dicts: Vec<crate::proxy::Dict>) -> String {
    Json::Array(dicts.into_iter().map(dict_to_json).collect()).to_string()
}

// ─── Search / listings / alternate sources ─────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn mx_store_search_packages(query: *const c_char, max: c_uint) -> *mut c_char {
    let Some((query, clients)) = cstr_to_str(query).zip(clients()) else {
        return ptr::null_mut();
    };
    match clients.store.search_packages(query, max) {
        Ok(dicts) => to_cstring(dicts_json(dicts)),
        Err(e) => {
            eprintln!("[store-client] search_packages: {e}");
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn mx_store_search_modules(query: *const c_char, max: c_uint) -> *mut c_char {
    let Some((query, clients)) = cstr_to_str(query).zip(clients()) else {
        return ptr::null_mut();
    };
    match clients.store.search_modules(query, max) {
        Ok(dicts) => to_cstring(dicts_json(dicts)),
        Err(e) => {
            eprintln!("[store-client] search_modules: {e}");
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn mx_store_list_installed_packages() -> *mut c_char {
    let Some(clients) = clients() else {
        return ptr::null_mut();
    };
    match clients.store.list_installed_packages() {
        Ok(dicts) => to_cstring(dicts_json(dicts)),
        Err(e) => {
            eprintln!("[store-client] list_installed_packages: {e}");
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn mx_store_list_installed_modules() -> *mut c_char {
    let Some(clients) = clients() else {
        return ptr::null_mut();
    };
    match clients.store.list_installed_modules() {
        Ok(dicts) => to_cstring(dicts_json(dicts)),
        Err(e) => {
            eprintln!("[store-client] list_installed_modules: {e}");
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn mx_store_list_module_plugins(name: *const c_char) -> *mut c_char {
    let Some((name, clients)) = cstr_to_str(name).zip(clients()) else {
        return ptr::null_mut();
    };
    match clients.store.list_module_plugins(name) {
        Ok(dicts) => to_cstring(dicts_json(dicts)),
        Err(e) => {
            eprintln!("[store-client] list_module_plugins: {e}");
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn mx_store_packages_for_app_id(app_id: *const c_char) -> *mut c_char {
    let Some((app_id, clients)) = cstr_to_str(app_id).zip(clients()) else {
        return ptr::null_mut();
    };
    match clients.store.packages_for_app_id(app_id) {
        Ok(dicts) => to_cstring(dicts_json(dicts)),
        Err(e) => {
            eprintln!("[store-client] packages_for_app_id: {e}");
            ptr::null_mut()
        }
    }
}

// ─── Flathub enrichment ─────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn mx_store_get_app_enrichment(app_id: *const c_char) -> *mut c_char {
    let Some((app_id, clients)) = cstr_to_str(app_id).zip(clients()) else {
        return ptr::null_mut();
    };
    match clients.store.get_app_enrichment(vec![app_id]) {
        Ok(mut map) => match map.remove(app_id) {
            Some(dict) => to_cstring(dict_to_json(dict).to_string()),
            None => ptr::null_mut(),
        },
        Err(e) => {
            eprintln!("[store-client] get_app_enrichment: {e}");
            ptr::null_mut()
        }
    }
}

/// Batched [`mx_store_get_app_enrichment`]: JSON `{ "<app_id>": {…}, … }`
/// covering every id the daemon had something for (others are omitted).
///
/// # Safety
/// `app_ids` must be NULL, or point to at least `n` valid `*const c_char`
/// entries, each NULL or a valid NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mx_store_get_app_enrichment_many(
    app_ids: *const *const c_char,
    n: c_uint,
) -> *mut c_char {
    // SAFETY: caller upholds the same contract as this function's.
    let ids = unsafe { collect_strv(app_ids, n) };
    let Some(clients) = (!ids.is_empty()).then(clients).flatten() else {
        return ptr::null_mut();
    };
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    match clients.store.get_app_enrichment(refs) {
        Ok(map) => {
            let obj: serde_json::Map<String, Json> = map
                .into_iter()
                .map(|(id, dict)| (id, dict_to_json(dict)))
                .collect();
            to_cstring(Json::Object(obj).to_string())
        }
        Err(e) => {
            eprintln!("[store-client] get_app_enrichment_many: {e}");
            ptr::null_mut()
        }
    }
}

// ─── Licenses ───────────────────────────────────────────────────────────────

/// SPDX expression of each nixpkgs attribute in `attrs`, as the JSON object
/// `{ "<attr>": "<spdx>", … }` — attributes the daemon found no license for
/// are omitted. Returns NULL on failure.
///
/// # Safety
/// `attrs` must be NULL, or point to at least `n` valid `*const c_char`
/// entries, each NULL or a valid NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mx_store_package_licenses(
    attrs: *const *const c_char,
    n: c_uint,
) -> *mut c_char {
    // SAFETY: caller upholds the same contract as this function's.
    let names = unsafe { collect_strv(attrs, n) };
    let Some(clients) = (!names.is_empty()).then(clients).flatten() else {
        return ptr::null_mut();
    };
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    match clients.store.get_package_licenses(refs) {
        Ok(map) => {
            let obj: serde_json::Map<String, Json> =
                map.into_iter().map(|(a, l)| (a, Json::from(l))).collect();
            to_cstring(Json::Object(obj).to_string())
        }
        Err(e) => {
            eprintln!("[store-client] get_package_licenses: {e}");
            ptr::null_mut()
        }
    }
}

// ─── Lifecycle (install / uninstall) ───────────────────────────────────────

/// # Safety
/// `names` must be NULL, or point to at least `n` valid `*const c_char`
/// entries, each NULL or a valid NUL-terminated string.
unsafe fn lifecycle_names(
    names: *const *const c_char,
    n: c_uint,
    call: impl FnOnce(Vec<&str>) -> zbus::Result<String>,
) -> *mut c_char {
    // SAFETY: caller upholds the same contract as this function's.
    let owned = unsafe { collect_strv(names, n) };
    if owned.is_empty() {
        return ptr::null_mut();
    }
    let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    match call(refs) {
        Ok(status) => to_cstring(status),
        Err(e) => {
            eprintln!("[store-client] lifecycle: {e}");
            ptr::null_mut()
        }
    }
}

/// # Safety
/// `names` must be NULL, or point to at least `n` valid `*const c_char`
/// entries, each NULL or a valid NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mx_store_install_packages(
    names: *const *const c_char,
    n: c_uint,
) -> *mut c_char {
    let Some(clients) = clients() else {
        return ptr::null_mut();
    };
    // SAFETY: forwarded from this function's own contract.
    unsafe { lifecycle_names(names, n, |names| clients.daemon.install_package(names)) }
}

/// # Safety
/// Same contract as [`mx_store_install_packages`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mx_store_uninstall_packages(
    names: *const *const c_char,
    n: c_uint,
) -> *mut c_char {
    let Some(clients) = clients() else {
        return ptr::null_mut();
    };
    // SAFETY: forwarded from this function's own contract.
    unsafe { lifecycle_names(names, n, |names| clients.daemon.uninstall_package(names)) }
}

/// # Safety
/// Same contract as [`mx_store_install_packages`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mx_store_install_modules(
    names: *const *const c_char,
    n: c_uint,
) -> *mut c_char {
    let Some(clients) = clients() else {
        return ptr::null_mut();
    };
    // SAFETY: forwarded from this function's own contract.
    unsafe { lifecycle_names(names, n, |names| clients.daemon.install_module(names)) }
}

/// # Safety
/// Same contract as [`mx_store_install_packages`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mx_store_uninstall_modules(
    names: *const *const c_char,
    n: c_uint,
) -> *mut c_char {
    let Some(clients) = clients() else {
        return ptr::null_mut();
    };
    // SAFETY: forwarded from this function's own contract.
    unsafe { lifecycle_names(names, n, |names| clients.daemon.uninstall_module(names)) }
}

#[unsafe(no_mangle)]
pub extern "C" fn mx_store_install_plugin(
    module: *const c_char,
    plugin: *const c_char,
) -> *mut c_char {
    let Some(((module, plugin), clients)) =
        cstr_to_str(module).zip(cstr_to_str(plugin)).zip(clients())
    else {
        return ptr::null_mut();
    };
    match clients.daemon.install_plugin(module, plugin) {
        Ok(status) => to_cstring(status),
        Err(e) => {
            eprintln!("[store-client] install_plugin: {e}");
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn mx_store_uninstall_plugin(
    module: *const c_char,
    plugin: *const c_char,
) -> *mut c_char {
    let Some(((module, plugin), clients)) =
        cstr_to_str(module).zip(cstr_to_str(plugin)).zip(clients())
    else {
        return ptr::null_mut();
    };
    match clients.daemon.uninstall_plugin(module, plugin) {
        Ok(status) => to_cstring(status),
        Err(e) => {
            eprintln!("[store-client] uninstall_plugin: {e}");
            ptr::null_mut()
        }
    }
}
