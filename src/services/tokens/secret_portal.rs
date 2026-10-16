use std::{
    env, ffi::OsString, future::Future, io::Read, os::unix::net::UnixStream, path::Path,
    time::Duration,
};

use ashpd::desktop::secret::Secret as PortalClient;
use gio::glib::{self, VariantTy};
use gio::prelude::*;
use gio::{self, DBusCallFlags, DBusProxyFlags};
use thiserror::Error;
use tracing::debug;

use crate::runtime::handle;

const PORTAL_BUS_NAME: &str = "org.freedesktop.portal.Desktop";
const PORTAL_OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
const INTROSPECT_INTERFACE: &str = "org.freedesktop.DBus.Introspectable";
const SECRET_INTERFACE: &str = "org.freedesktop.portal.Secret";
const INTROSPECT_TIMEOUT_MS: i32 = 5_000;
const SECRET_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const ENABLE_ENV: &str = "ACTIONEER_ENABLE_SECRET_PORTAL";
const DISABLE_ENV: &str = "ACTIONEER_DISABLE_SECRET_PORTAL";
/// Present in every Flatpak sandbox; the environment may be stripped, the file
/// cannot. libsecret checks the same path.
const FLATPAK_MARKER_FILE: &str = "/.flatpak-info";

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
    #[error("secret portal request timed out after {0:?}")]
    Timeout(Duration),
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

    let xml_variant = if xml_variant.is_type(VariantTy::STRING) {
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
    handle().block_on(run_portal_request_with_timeout(
        || async {
            let portal = PortalClient::new()
                .await
                .map_err(PortalSecretError::Portal)?;
            let request = portal
                .retrieve(&writer, Default::default())
                .await
                .map_err(PortalSecretError::Portal)?;
            request.response().map_err(PortalSecretError::Portal)?;
            Ok::<(), PortalSecretError>(())
        },
        SECRET_REQUEST_TIMEOUT,
    ))?;
    drop(writer);

    let secret_bytes = read_secret(reader)?;
    if secret_bytes.is_empty() {
        return Err(PortalSecretError::InvalidResponse);
    }

    Ok(PortalSecret {
        secret: secret_bytes,
    })
}

/// Returns the environment or filesystem marker that marks a confined runtime.
fn sandbox_reason() -> Option<&'static str> {
    sandbox_reason_impl(|name| env::var_os(name), Path::new(FLATPAK_MARKER_FILE))
}

/// Testable core of [`sandbox_reason`]: `lookup` resolves environment variables
/// and `flatpak_marker` stands in for the runtime's marker file, checked last
/// so environment markers keep their precedence.
fn sandbox_reason_impl(
    lookup: impl Fn(&str) -> Option<OsString>,
    flatpak_marker: &Path,
) -> Option<&'static str> {
    const CANDIDATES: [(&str, &str); 4] = [
        ("SNAP", "SNAP"),
        ("SNAP_INSTANCE_NAME", "SNAP_INSTANCE_NAME"),
        ("SNAP_NAME", "SNAP_NAME"),
        ("FLATPAK_ID", "FLATPAK_ID"),
    ];

    for (var, label) in CANDIDATES {
        if lookup(var).is_some() {
            return Some(label);
        }
    }

    if let Some(value) = lookup("CONTAINER").and_then(|value| value.into_string().ok())
        && (value.eq_ignore_ascii_case("flatpak") || value.eq_ignore_ascii_case("snap"))
    {
        return Some("CONTAINER");
    }

    if flatpak_marker.exists() {
        return Some(FLATPAK_MARKER_FILE);
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
    matches!(
        value,
        "1" | "true" | "TRUE" | "True" | "yes" | "YES" | "Yes" | "on" | "ON" | "On"
    )
}

async fn run_portal_request_with_timeout<F, Fut>(
    request: F,
    timeout: Duration,
) -> Result<(), PortalSecretError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<(), PortalSecretError>>,
{
    tokio::time::timeout(timeout, request())
        .await
        .map_err(|_| PortalSecretError::Timeout(timeout))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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

    #[tokio::test]
    async fn portal_request_timeout_returns_explicit_error() {
        let err = run_portal_request_with_timeout(
            || async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(())
            },
            Duration::from_millis(1),
        )
        .await
        .expect_err("request should time out");

        assert!(
            matches!(err, PortalSecretError::Timeout(timeout) if timeout == Duration::from_millis(1))
        );
    }

    #[tokio::test]
    async fn portal_request_timeout_allows_fast_requests() {
        run_portal_request_with_timeout(|| async { Ok(()) }, Duration::from_secs(1))
            .await
            .expect("request should succeed");
    }

    #[test]
    fn sandbox_reason_reports_environment_markers() {
        let env = |name: &str| (name == "SNAP_NAME").then(|| OsString::from("actioneer"));
        assert_eq!(
            sandbox_reason_impl(env, Path::new("/nonexistent")),
            Some("SNAP_NAME")
        );
    }

    #[test]
    fn sandbox_reason_reports_container_values() {
        let env = |name: &str| (name == "CONTAINER").then(|| OsString::from("FlatPak"));
        assert_eq!(
            sandbox_reason_impl(env, Path::new("/nonexistent")),
            Some("CONTAINER")
        );
    }

    #[test]
    fn sandbox_reason_reports_the_flatpak_marker_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let marker = dir.path().join("flatpak-info");
        std::fs::write(&marker, b"").expect("write marker file");

        assert_eq!(
            sandbox_reason_impl(|_| None, &marker),
            Some(FLATPAK_MARKER_FILE)
        );
    }

    #[test]
    fn sandbox_reason_prefers_environment_markers_over_the_marker_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let marker = dir.path().join("flatpak-info");
        std::fs::write(&marker, b"").expect("write marker file");

        let lookup = |name: &str| (name == "FLATPAK_ID").then(|| OsString::from("1"));
        assert_eq!(sandbox_reason_impl(lookup, &marker), Some("FLATPAK_ID"));
    }

    #[test]
    fn sandbox_reason_ignores_non_unicode_container_values() {
        use std::os::unix::ffi::OsStringExt;

        let lookup = |name: &str| (name == "CONTAINER").then(|| OsString::from_vec(vec![0xff]));
        assert_eq!(sandbox_reason_impl(lookup, Path::new("/nonexistent")), None);
    }

    #[test]
    fn sandbox_reason_is_none_without_markers() {
        assert_eq!(
            sandbox_reason_impl(|_| None, Path::new("/nonexistent")),
            None
        );
    }
}
