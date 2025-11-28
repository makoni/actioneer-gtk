use std::{
    cell::{Cell, RefCell},
    env,
    io::Read,
    os::unix::net::UnixStream,
    rc::Rc,
};

use gio::glib::{self, ControlFlow, Variant, VariantTy};
use gio::prelude::*;
use gio::{self, DBusCallFlags, DBusProxyFlags, UnixFDList};
use glib::variant::{Handle, ObjectPath, VariantTypeMismatchError};
use thiserror::Error;
use tracing::debug;

const PORTAL_BUS_NAME: &str = "org.freedesktop.portal.Desktop";
const PORTAL_OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
const INTROSPECT_INTERFACE: &str = "org.freedesktop.DBus.Introspectable";
const SECRET_INTERFACE: &str = "org.freedesktop.portal.Secret";
const REQUEST_INTERFACE: &str = "org.freedesktop.portal.Request";
const RESPONSE_SIGNAL: &str = "Response";
const INTROSPECT_TIMEOUT_MS: i32 = 5_000;
const SECRET_RESPONSE_TIMEOUT_SECS: u32 = 5;
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
    #[error("failed to communicate with portal: {0}")]
    Dbus(#[from] glib::Error),
    #[error("failed to create UNIX stream: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to attach file descriptor for secret portal request: {0}")]
    Fd(glib::Error),
    #[error("secret portal response timed out")]
    Timeout,
    #[error("secret portal request failed with code {code}")]
    RequestFailed { code: u32 },
    #[error("secret portal response missing expected data")]
    InvalidResponse,
}

#[derive(Debug, Clone)]
pub struct PortalSecret {
    pub secret: Vec<u8>,
    pub token: Option<String>,
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
pub fn retrieve_secret(previous_token: Option<&str>) -> Result<PortalSecret, PortalSecretError> {
    let connection = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>)?;

    let proxy = gio::DBusProxy::new_sync(
        &connection,
        DBusProxyFlags::DO_NOT_AUTO_START,
        None::<&gio::DBusInterfaceInfo>,
        Some(PORTAL_BUS_NAME),
        PORTAL_OBJECT_PATH,
        SECRET_INTERFACE,
        None::<&gio::Cancellable>,
    )?;

    let (reader, writer) = UnixStream::pair()?;
    let fd_list = UnixFDList::new();
    let fd_index = fd_list.append(&writer).map_err(PortalSecretError::Fd)?;
    drop(writer);

    let options = build_options_variant(previous_token);
    let params = glib::Variant::tuple_from_iter([Handle::from(fd_index).to_variant(), options]);
    let (request_handle, _) = proxy.call_with_unix_fd_list_sync(
        "RetrieveSecret",
        Some(&params),
        DBusCallFlags::NONE,
        INTROSPECT_TIMEOUT_MS,
        Some(&fd_list),
        None::<&gio::Cancellable>,
    )?;

    let request_path: ObjectPath = request_handle
        .get()
        .ok_or(PortalSecretError::InvalidResponse)?;
    let (status, results) = wait_for_portal_response(&connection, request_path.as_str())?;

    if status != 0 {
        return Err(PortalSecretError::RequestFailed { code: status });
    }

    let secret_bytes = read_secret(reader)?;
    if secret_bytes.is_empty() {
        return Err(PortalSecretError::InvalidResponse);
    }

    let token = extract_response_token(&results)?;

    Ok(PortalSecret {
        secret: secret_bytes,
        token,
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

fn build_options_variant(previous_token: Option<&str>) -> Variant {
    let dict = glib::VariantDict::new(None);
    if let Some(token) = previous_token {
        dict.insert("token", token);
    }
    dict.to_variant()
}

fn wait_for_portal_response(
    connection: &gio::DBusConnection,
    request_path: &str,
) -> Result<(u32, Variant), PortalSecretError> {
    let context = glib::MainContext::new();
    let loop_ = glib::MainLoop::new(Some(&context), false);
    let response = Rc::new(RefCell::new(None));
    let timed_out = Rc::new(Cell::new(false));

    context
        .with_thread_default(|| {
            let loop_clone = loop_.clone();
            let response_clone = Rc::clone(&response);
            let subscription = connection.subscribe_to_signal(
                Some(PORTAL_BUS_NAME),
                Some(REQUEST_INTERFACE),
                Some(RESPONSE_SIGNAL),
                Some(request_path),
                None,
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    if let Some((code, results)) = signal.parameters.get::<(u32, Variant)>() {
                        response_clone.replace(Some((code, results)));
                    }
                    loop_clone.quit();
                },
            );

            let timeout_loop = loop_.clone();
            let timed_out_clone = Rc::clone(&timed_out);
            let timeout_source =
                glib::timeout_add_seconds_local(SECRET_RESPONSE_TIMEOUT_SECS, move || {
                    timed_out_clone.set(true);
                    timeout_loop.quit();
                    ControlFlow::Break
                });

            loop_.run();

            timeout_source.remove();
            drop(subscription);
        })
        .map_err(|_| PortalSecretError::InvalidResponse)?;

    if timed_out.get() {
        return Err(PortalSecretError::Timeout);
    }

    response
        .borrow()
        .clone()
        .ok_or(PortalSecretError::InvalidResponse)
}

fn read_secret(mut reader: UnixStream) -> Result<Vec<u8>, PortalSecretError> {
    let mut bytes = Vec::with_capacity(64);
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn extract_response_token(results: &Variant) -> Result<Option<String>, PortalSecretError> {
    let dict = glib::VariantDict::new(Some(results));
    match dict.lookup::<String>("token") {
        Ok(value) => Ok(value),
        Err(VariantTypeMismatchError { .. }) => Err(PortalSecretError::InvalidResponse),
    }
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
