/// Application configuration loaded from environment variables
///
/// Configuration is loaded from .env file if it exists, or from system environment.
/// This allows developers to use their own GitHub OAuth app credentials.
use std::sync::OnceLock;

static CONFIG: OnceLock<Config> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct Config {
    pub github_client_id: String,
    pub github_client_secret: Option<String>,
}

impl Config {
    /// Load configuration from environment
    fn load() -> Self {
        // Try to load .env file (silently fail if not present)
        let _ = dotenvy::dotenv();

        let github_client_id = std::env::var("GITHUB_CLIENT_ID").unwrap_or_else(|_| {
            eprintln!("Warning: GITHUB_CLIENT_ID not set, using default");
            "Ov23libpEp1giiQpBYpe".to_string()
        });

        let github_client_secret = std::env::var("GITHUB_CLIENT_SECRET").ok();

        Self {
            github_client_id,
            github_client_secret,
        }
    }

    /// Get the global configuration instance
    pub fn get() -> &'static Config {
        CONFIG.get_or_init(Self::load)
    }

    /// Get GitHub client ID
    pub fn github_client_id() -> &'static str {
        &Self::get().github_client_id
    }

    /// Get GitHub client secret if available
    pub fn github_client_secret() -> Option<&'static str> {
        Self::get().github_client_secret.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_has_client_id() {
        let config = Config::get();
        assert!(!config.github_client_id.is_empty());
        // Touch optional secret accessor so it remains in active use even when unset.
        let _ = Config::github_client_secret();
    }
}
