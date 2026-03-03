use std::env;

pub fn initialize_portal_env(resolved_app_id: &str) {
    apply_portal_env_overrides(resolved_app_id);
}

pub(super) fn is_sandboxed() -> bool {
    env::var_os("FLATPAK_ID").is_some()
        || env::var_os("SNAP").is_some()
        || env::var_os("APPIMAGE").is_some()
}

pub(super) fn resolve_portal_app_id(default_app_id: &str) -> String {
    if env::var_os("SNAP").is_some() {
        // Snapd exports desktop files as <snap_name>_<desktop_id>.desktop. Align the portal
        // app-id with that basename so the portal can match the installed desktop file.
        let snap_name = env::var("SNAP_INSTANCE_NAME")
            .or_else(|_| env::var("SNAP_NAME"))
            .unwrap_or_else(|_| "snap".to_string());

        let prefixed = format!("{}_", snap_name);
        if default_app_id.starts_with(&prefixed) {
            return default_app_id.to_string();
        }

        return format!("{}{}", prefixed, default_app_id);
    }

    default_app_id.to_string()
}

fn apply_portal_env_overrides(resolved_app_id: &str) {
    let force_key = "XDG_DESKTOP_PORTAL_FORCE_USE_THIS_APP_ID";
    if env::var_os(force_key).is_none() {
        // SAFETY: startup-only process-local env var mutation.
        unsafe {
            env::set_var(force_key, resolved_app_id);
        }
    }

    let app_id_key = "XDG_DESKTOP_PORTAL_APP_ID";
    if env::var_os(app_id_key).is_none() {
        // SAFETY: startup-only process-local env var mutation.
        unsafe {
            env::set_var(app_id_key, resolved_app_id);
        }
    }

    // For snaps, also provide the desktop file hint expected by portals.
    if env::var_os("SNAP").is_some()
        && env::var_os("XDG_DESKTOP_PORTAL_USE_THIS_DESKTOP_ID").is_none()
    {
        let desktop_id = format!("{}.desktop", resolved_app_id);
        unsafe {
            env::set_var("XDG_DESKTOP_PORTAL_USE_THIS_DESKTOP_ID", desktop_id);
        }
    }
}
