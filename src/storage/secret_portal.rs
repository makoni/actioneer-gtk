use std::{env, io::Read, os::unix::net::UnixStream};

use ashpd::desktop::secret::Secret as PortalClient;
use gio::glib::{self, VariantTy};
use gio::prelude::*;
use gio::{self, DBusCallFlags, DBusProxyFlags};
use thiserror::Error;
use tracing::debug;

use crate::runtime_handle;

const PORTAL_BUS_NAME: &str = "org.freedesktop.portal.Desktop";
const PORTAL_OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
const INTROSPECT_INTERFACE: &str = "org.freedesktop.DBus.Introspectable";
const SECRET_INTERFACE: &str = "org.freedesktop.portal.Secret";
const INTROSPECT_TIMEOUT_MS: i32 = 5_000;
const ENABLE_ENV: &str = "ACTIONEER_ENABLE_SECRET_PORTAL";
const DISABLE_ENV: &str = "ACTIONEER_DISABLE_SECRET_PORTAL";

#[derive(Debug, Error)]
pub enum PortalDetectionError {
    #[error("failed to connect to session bus: {0}")]
    Bus(#[from] glib::Error),

    #[error("portal introspection response was not a string")]
    UnexpectedIntrospection,
}

#[derive(Debug, Error)]
pub enum PortalSecretError {
    #[error("failed to create UNIX stream: {0}")]
    Io(#[from] std::io::Error),
    #[error("secret portal request failed: {0}")]
    Portal(ashpd::Error),
    #[error("secret portal response missing expected data")]
    InvalidResponse,
}

#[derive(Debug, Clone)]
pub struct PortalSecret {
    pub secret: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub enum PortalPreference {
    ForcedEnabled(&'static str),
    ForcedDisabled(&'static str),
    Sandboxed(&'static str),
    Disabled,
}

impl PortalPreference {
    pub fn use_portal(self) -> bool {
        matches!(
            self,
            PortalPreference::ForcedEnabled(_) | PortalPreference::Sandboxed(_)
        )
    }

    pub fn describe(self) -> &'static str {
        match self {
            PortalPreference::ForcedEnabled(reason)
            | PortalPreference::ForcedDisabled(reason)
            | PortalPreference::Sandboxed(reason) => reason,
            PortalPreference::Disabled => "desktop session",
        }
    }
}

/// Returns the current preference for using the secret portal, taking environment overrides
/// and confinement hints into account.
pub fn portal_preference() -> PortalPreference {
    if env_flag(DISABLE_ENV) {
        return PortalPreference::ForcedDisabled(DISABLE_ENV);
    }

    if env_flag(ENABLE_ENV) {
        return PortalPreference::ForcedEnabled(ENABLE_ENV);
    }

    if let Some(reason) = sandbox_reason() {
        return PortalPreference::Sandboxed(reason);
    }

    PortalPreference::Disabled
}

/// Inspect the desktop portal to determine if the Secret interface is advertised.
pub fn secret_portal_available() -> Result<bool, PortalDetectionError> {
    let connection = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>)?;
    if let Some(name) = connection.unique_name() {
        debug!("Secret portal detection using connection {name}");
    }

    let proxy = gio::DBusProxy::new_sync(
        &connection,
        DBusProxyFlags::DO_NOT_AUTO_START | DBusProxyFlags::DO_NOT_LOAD_PROPERTIES,
        None::<&gio::DBusInterfaceInfo>,
        Some(PORTAL_BUS_NAME),
        PORTAL_OBJECT_PATH,
        INTROSPECT_INTERFACE,
        None::<&gio::Cancellable>,
    )?;

    let xml_variant = proxy.call_sync(
        "Introspect",
        None::<&glib::Variant>,
        DBusCallFlags::NONE,
        INTROSPECT_TIMEOUT_MS,
        None::<&gio::Cancellable>,
    )?;

    let xml_variant = if xml_variant.is_type(&VariantTy::STRING) {
        xml_variant
    } else {
        xml_variant.child_value(0)
    };
    let xml = xml_variant
        .str()
        .ok_or(PortalDetectionError::UnexpectedIntrospection)?;
    let available = xml.contains(SECRET_INTERFACE);

    if available {
        debug!("Found {SECRET_INTERFACE} via portal introspection");
    } else {
        debug!("{SECRET_INTERFACE} missing from portal introspection");
    }

    Ok(available)
}

/// Retrieve the per-application secret via the portal and return it alongside any session token.
pub fn retrieve_secret(_previous_token: Option<&str>) -> Result<PortalSecret, PortalSecretError> {
    let (reader, writer) = UnixStream::pair()?;
    runtime_handle().block_on(async {
        let portal = PortalClient::new()
            .await
            .map_err(PortalSecretError::Portal)?;
        let request = portal
            .retrieve(&writer)
            .await
            .map_err(PortalSecretError::Portal)?;
        request.response().map_err(PortalSecretError::Portal)?;
        Ok::<(), PortalSecretError>(())
    })?;
    drop(writer);

    let secret_bytes = read_secret(reader)?;
    if secret_bytes.is_empty() {
        return Err(PortalSecretError::InvalidResponse);
    }

    Ok(PortalSecret {
        secret: secret_bytes,
    })
}

fn sandbox_reason() -> Option<&'static str> {
    const CANDIDATES: [(&str, &str); 4] = [
        ("SNAP", "SNAP"),
        ("SNAP_INSTANCE_NAME", "SNAP_INSTANCE_NAME"),
        ("SNAP_NAME", "SNAP_NAME"),
        ("FLATPAK_ID", "FLATPAK_ID"),
    ];

    for (var, label) in CANDIDATES {
        if env::var_os(var).is_some() {
            return Some(label);
        }
    }

    if let Ok(value) = env::var("CONTAINER") {
        if value.eq_ignore_ascii_case("flatpak") || value.eq_ignore_ascii_case("snap") {
            return Some("CONTAINER");
        }
    }

    None
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .ok()
        .map(|value| parse_flag(&value))
        .unwrap_or(false)
}

fn read_secret(mut reader: UnixStream) -> Result<Vec<u8>, PortalSecretError> {
    let mut bytes = Vec::with_capacity(64);
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn parse_flag(value: &str) -> bool {
    match value {
        "1" | "true" | "TRUE" | "True" | "yes" | "YES" | "Yes" | "on" | "ON" | "On" => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_truthy_flags() {
        for value in [
            "1", "true", "TRUE", "True", "yes", "YES", "Yes", "on", "ON", "On",
        ] {
            assert!(
                parse_flag(value),
                "Expected '{value}' to be recognized as true"
            );
        }
        assert!(!parse_flag("0"));
        assert!(!parse_flag("false"));
        assert!(!parse_flag(""));
    }
}
