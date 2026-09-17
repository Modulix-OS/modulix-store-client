//! Client for `org.modulix.Daemon`'s two D-Bus interfaces (`Store1` reads,
//! `Daemon` writes) — no dependency on `modulix-core-utils`, which the daemon
//! now consumes exclusively.
//!
//! Two ways to use this crate:
//! - Rust consumers: the async [`StoreProxy`]/[`DaemonProxy`] (see [`proxy`]).
//! - C consumers (`gnome-software-plugin`): the blocking `mx_store_*` C-ABI
//!   in [`ffi`], which re-serializes `a{sv}` replies to JSON so the existing
//!   json-glib parsing on the C side needs no rewrite.

pub mod convert;
mod ffi;
pub mod proxy;

pub use ffi::{STORE_ERROR, STORE_OK, StoreResult};
pub use proxy::{DaemonProxy, Dict, StoreProxy};
