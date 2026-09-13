//! Infrastructure adapters: network, filesystem, D-Bus, secrets.
//!
//! Everything here talks to something outside the process. The layer above
//! (`ui/`) reaches these through the composition root; the layers below
//! (`kernel/`, `domain/`) never name them.

pub mod api;
pub mod app_services;
pub mod auth;
pub mod cache;
pub mod config;
pub mod crash_report;
pub mod error;
pub mod favorites;
pub mod gateway;
pub mod notifications;
pub mod preferences;
pub mod tokens;
