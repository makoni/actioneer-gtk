//! Application identity: the ids the app registers under and the version line.

use std::borrow::Cow;

pub const APP_ID: &str = "me.spaceinbox.actioneer";
pub const APP_ICON_NAME: &str = APP_ID;

/// One line naming this build and the GTK actually loaded at runtime.
///
/// The GTK numbers come from the library that answered the call, not from what
/// the crate was compiled against, so this is also how a packaged build — the
/// AppImage bundles its own GTK — reports what it really ships. libadwaita is
/// left out on purpose: its version functions abort unless GTK has been
/// initialised, and printing a version must not need a display.
pub fn version_string() -> String {
    format!(
        "actioneer {} (gtk {}.{}.{})",
        env!("CARGO_PKG_VERSION"),
        gtk4::major_version(),
        gtk4::minor_version(),
        gtk4::micro_version(),
    )
}

/// The application id to register under, namespaced when running inside a Snap.
pub fn resolved_app_id() -> Cow<'static, str> {
    if let Ok(snap_name) =
        std::env::var("SNAP_INSTANCE_NAME").or_else(|_| std::env::var("SNAP_NAME"))
    {
        Cow::Owned(format!("{}_{}", snap_name, APP_ID))
    } else {
        Cow::Borrowed(APP_ID)
    }
}
