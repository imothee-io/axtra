//! cmdline.io error tracking provider.

use reqwest::Client;

use crate::notifier::{ErrorEvent, ErrorNotifier, NotifyFuture};

/// Configuration for the cmdline.io error tracking provider.
#[derive(Debug, Clone)]
pub struct CmdlineConfig {
    /// Base URL (default: `https://cmdline.io`)
    pub base_url: String,
    /// API key for authentication (required)
    pub api_key: String,
    /// Environment name (e.g., "production", "staging")
    pub environment: Option<String>,
    /// Service/app name (e.g., "api", "web", "worker")
    pub service: Option<String>,
    /// Release/version (e.g., "1.2.3", commit hash)
    pub release: Option<String>,
}

impl CmdlineConfig {
    /// Create a new config with the default cmdline.io server.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            base_url: "https://cmdline.io".into(),
            api_key: api_key.into(),
            environment: None,
            service: None,
            release: None,
        }
    }

    /// Set a custom base URL.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the environment name.
    pub fn with_environment(mut self, env: impl Into<String>) -> Self {
        self.environment = Some(env.into());
        self
    }

    /// Set the service name.
    pub fn with_service(mut self, service: impl Into<String>) -> Self {
        self.service = Some(service.into());
        self
    }

    /// Set the release version.
    pub fn with_release(mut self, release: impl Into<String>) -> Self {
        self.release = Some(release.into());
        self
    }
}

/// cmdline.io error tracking provider.
///
/// Sends errors to cmdline.io for tracking and deduplication. Errors are
/// automatically grouped by fingerprint (auto-generated from title + stacktrace
/// if not provided).
///
/// Severity mapping from `ErrorCode`:
/// - `Database` → `fatal`
/// - `Exception` → `error`
/// - `Authentication`, `Authorization` → `warning`
/// - `BadRequest`, `NotFound`, `Validation` → `warning`
///
/// # Example
///
/// ```rust,ignore
/// use axtra::notifier::{NotificationManager, CmdlineConfig};
///
/// let manager = NotificationManager::builder()
///     .with_cmdline(
///         CmdlineConfig::new(std::env::var("CMDLINE_API_KEY").unwrap())
///             .with_environment("production")
///             .with_service("api")
///     )
///     .build();
/// ```
pub struct CmdlineProvider {
    config: CmdlineConfig,
}

impl CmdlineProvider {
    /// Create a new cmdline.io provider with the given configuration.
    pub fn new(config: CmdlineConfig) -> Self {
        Self { config }
    }
}

impl ErrorNotifier for CmdlineProvider {
    fn notify<'a>(&'a self, client: &'a Client, event: &'a ErrorEvent) -> NotifyFuture<'a> {
        Box::pin(async move {
            let url = format!(
                "{}/api/ingest/errors",
                self.config.base_url.trim_end_matches('/')
            );

            // Build title: "ErrorType: message" (max 500 chars)
            let title = format!("{:?}: {}", event.error_code, event.message);
            let title = if title.len() > 500 {
                format!("{}...", &title[..497])
            } else {
                title
            };

            // Map error codes to API severity levels
            let severity = match event.error_code {
                crate::errors::ErrorCode::Database => "fatal",
                crate::errors::ErrorCode::Exception => "error",
                crate::errors::ErrorCode::Authentication
                | crate::errors::ErrorCode::Authorization => "warning",
                crate::errors::ErrorCode::BadRequest
                | crate::errors::ErrorCode::NotFound
                | crate::errors::ErrorCode::Validation => "warning",
            };

            // Determine error type
            let error_type = match event.error_code {
                crate::errors::ErrorCode::BadRequest
                | crate::errors::ErrorCode::NotFound
                | crate::errors::ErrorCode::Authentication
                | crate::errors::ErrorCode::Authorization => "http_error",
                _ => "exception",
            };

            let mut payload = serde_json::json!({
                "title": title,
                "severity": severity,
                "error_type": error_type,
                "service": event.app_name,
            });

            // Add optional message (full error)
            if let Some(ref source) = event.source_error {
                payload["message"] = serde_json::Value::String(source.clone());
            }

            // Add location as extra context
            payload["extra"] = serde_json::json!({
                "location": event.location,
            });

            // Add config-level defaults
            if let Some(ref env) = self.config.environment {
                payload["environment"] = serde_json::Value::String(env.clone());
            }
            if let Some(ref service) = self.config.service {
                // Config service overrides event app_name
                payload["service"] = serde_json::Value::String(service.clone());
            }
            if let Some(ref release) = self.config.release {
                payload["release"] = serde_json::Value::String(release.clone());
            }

            // Add event-level context if present
            if let Some(ref stacktrace) = event.stacktrace {
                payload["stacktrace"] = serde_json::Value::String(stacktrace.clone());
            }
            if let Some(ref request_url) = event.request_url {
                payload["request_url"] = serde_json::Value::String(request_url.clone());
            }
            if let Some(ref request_method) = event.request_method {
                payload["request_method"] = serde_json::Value::String(request_method.clone());
            }
            if let Some(ref user_id) = event.user_id {
                payload["user_id"] = serde_json::Value::String(user_id.clone());
            }
            if let Some(ref user_email) = event.user_email {
                payload["user_email"] = serde_json::Value::String(user_email.clone());
            }
            if let Some(ref tags) = event.tags {
                payload["tags"] = tags.clone();
            }
            if let Some(ref extra) = event.extra {
                // Merge with existing extra (location)
                if let Some(existing) = payload.get_mut("extra")
                    && let (Some(existing_obj), Some(new_obj)) =
                        (existing.as_object_mut(), extra.as_object())
                {
                    for (k, v) in new_obj {
                        existing_obj.insert(k.clone(), v.clone());
                    }
                }
            }
            if let Some(ref breadcrumbs) = event.breadcrumbs {
                payload["breadcrumbs"] = breadcrumbs.clone();
            }
            if let Some(ref fingerprint) = event.fingerprint {
                payload["fingerprint"] = serde_json::Value::String(fingerprint.clone());
            }

            client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .header("Content-Type", "application/json")
                .json(&payload)
                .send()
                .await?
                .error_for_status()?;

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "cmdline"
    }
}
