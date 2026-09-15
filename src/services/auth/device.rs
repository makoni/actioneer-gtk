use crate::services::config::Config;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info, warn};

const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const ACCESS_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("Authorization pending")]
    AuthorizationPending,

    #[error("Slow down polling")]
    SlowDown,

    #[error("Device code expired")]
    ExpiredToken,

    #[error("Access denied by user")]
    AccessDenied,

    #[error("Unknown error: {0}")]
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceFlowInfo {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: i64,
    pub interval: i64,
}

#[derive(Debug, Serialize)]
struct DeviceCodeRequest {
    client_id: String,
    scope: String,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: i64,
    interval: i64,
}

#[derive(Debug, Serialize)]
struct AccessTokenRequest {
    client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_secret: Option<String>,
    device_code: String,
    grant_type: String,
}

#[derive(Debug, Deserialize)]
struct AccessTokenResponse {
    access_token: Option<String>,
    token_type: Option<String>,
    scope: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AccessToken {
    pub token: String,
    pub token_type: String,
    pub scope: String,
}

/// Start the device flow authorization process
pub async fn start_device_flow(
    client_id: &str,
    scopes: &[&str],
) -> Result<DeviceFlowInfo, AuthError> {
    let client = reqwest::Client::new();
    let scope = scopes.join(" ");

    info!("Starting device flow with scopes: {}", scope);

    let request = DeviceCodeRequest {
        client_id: client_id.to_string(),
        scope,
    };

    let response = client
        .post(DEVICE_CODE_URL)
        .header("Accept", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await?;
        warn!("Device code request failed: {} - {}", status, text);
        return Err(AuthError::Unknown(format!("Status {}: {}", status, text)));
    }

    let device_response: DeviceCodeResponse = response.json().await?;

    debug!(
        "Device flow started: user_code={}, expires_in={}s",
        device_response.user_code, device_response.expires_in
    );

    Ok(DeviceFlowInfo {
        device_code: device_response.device_code,
        user_code: device_response.user_code,
        verification_uri: device_response.verification_uri,
        expires_in: device_response.expires_in,
        interval: device_response.interval,
    })
}

/// Poll for access token
pub async fn poll_device_token(
    client_id: &str,
    device_code: &str,
) -> Result<AccessToken, AuthError> {
    let client = reqwest::Client::new();

    let client_secret = Config::github_client_secret().map(str::to_string);

    let request = AccessTokenRequest {
        client_id: client_id.to_string(),
        client_secret,
        device_code: device_code.to_string(),
        grant_type: "urn:ietf:params:oauth:grant-type:device_code".to_string(),
    };

    let response = client
        .post(ACCESS_TOKEN_URL)
        .header("Accept", "application/json")
        .json(&request)
        .send()
        .await?;

    let token_response: AccessTokenResponse = response.json().await?;

    if let Some(error) = token_response.error {
        return match error.as_str() {
            "authorization_pending" => {
                debug!("Authorization pending, continue polling");
                Err(AuthError::AuthorizationPending)
            }
            "slow_down" => {
                debug!("Slow down requested");
                Err(AuthError::SlowDown)
            }
            "expired_token" => {
                warn!("Device code expired");
                Err(AuthError::ExpiredToken)
            }
            "access_denied" => {
                warn!("Access denied by user");
                Err(AuthError::AccessDenied)
            }
            _ => {
                warn!("Unknown error: {}", error);
                Err(AuthError::Unknown(error))
            }
        };
    }

    if let Some(access_token) = token_response.access_token {
        info!("Successfully obtained access token");
        Ok(AccessToken {
            token: access_token,
            token_type: token_response
                .token_type
                .unwrap_or_else(|| "bearer".to_string()),
            scope: token_response.scope.unwrap_or_default(),
        })
    } else {
        Err(AuthError::Unknown(
            "No access token in response".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_device_flow_structures() {
        // Test that our structures serialize/deserialize correctly
        let info = DeviceFlowInfo {
            device_code: "test_device_code".to_string(),
            user_code: "ABCD-1234".to_string(),
            verification_uri: "https://github.com/login/device".to_string(),
            expires_in: 900,
            interval: 5,
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: DeviceFlowInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(info.user_code, deserialized.user_code);
        assert_eq!(info.expires_in, deserialized.expires_in);
    }
}
