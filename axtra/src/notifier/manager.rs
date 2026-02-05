//! Notification manager for coordinating multiple providers.

use reqwest::Client;
use std::sync::Arc;

use super::{ErrorEvent, ErrorNotifier};

#[cfg(feature = "notify-error-slack")]
use super::providers::SlackProvider;
#[cfg(feature = "notify-error-discord")]
use super::providers::DiscordProvider;
#[cfg(feature = "notify-error-ntfy")]
use super::providers::NtfyProvider;
#[cfg(feature = "notify-error-cmdline")]
use super::providers::CmdlineProvider;

/// Manages multiple notification providers and dispatches error events to all of them.
///
/// # Example
///
/// ```rust,ignore
/// use axtra::notifier::{NotificationManager, NtfyConfig};
///
/// let manager = NotificationManager::builder()
///     .with_slack("https://hooks.slack.com/services/...")
///     .with_ntfy(NtfyConfig {
///         server_url: "https://ntfy.sh".into(),
///         topic: "my-app-errors".into(),
///         access_token: None,
///     })
///     .build();
///
/// // Or auto-configure from environment variables:
/// let manager = NotificationManager::from_env();
/// ```
pub struct NotificationManager {
    providers: Vec<Arc<dyn ErrorNotifier>>,
    client: Client,
}

impl NotificationManager {
    /// Create a new notification manager with no providers.
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            client: Client::new(),
        }
    }

    /// Create a builder for configuring the notification manager.
    pub fn builder() -> NotificationManagerBuilder {
        NotificationManagerBuilder::new()
    }

    /// Create a notification manager from environment variables.
    ///
    /// This will automatically configure providers based on the following env vars:
    /// - `SLACK_ERROR_WEBHOOK_URL` - Slack webhook URL
    /// - `DISCORD_ERROR_WEBHOOK_URL` - Discord webhook URL
    /// - `NTFY_TOPIC` - ntfy topic (uses `NTFY_SERVER_URL` for custom server, `NTFY_ACCESS_TOKEN` for auth)
    /// - `CMDLINE_API_KEY` - cmdline.io API key (required)
    /// - `ENVIRONMENT` or `ENV` - environment name like "production" (optional)
    /// - `SERVICE` or `APP_NAME` - service name like "api" (optional)
    /// - `RELEASE` or `VERSION` - release version (optional)
    pub fn from_env() -> Self {
        let mut builder = Self::builder();

        #[cfg(feature = "notify-error-slack")]
        if let Ok(url) = std::env::var("SLACK_ERROR_WEBHOOK_URL") {
            use super::providers::SlackConfig;
            let mut config = SlackConfig::new(url);
            if let Ok(mention) = std::env::var("SLACK_ERROR_MENTION") {
                config = config.with_mention(mention);
            }
            builder = builder.with_slack(config);
        }

        #[cfg(feature = "notify-error-discord")]
        if let Ok(url) = std::env::var("DISCORD_ERROR_WEBHOOK_URL") {
            use super::providers::DiscordConfig;
            let mut config = DiscordConfig::new(url);
            if let Ok(mention) = std::env::var("DISCORD_ERROR_MENTION") {
                config = config.with_mention(mention);
            }
            builder = builder.with_discord(config);
        }

        #[cfg(feature = "notify-error-ntfy")]
        if let Ok(topic) = std::env::var("NTFY_TOPIC") {
            use super::providers::NtfyConfig;
            builder = builder.with_ntfy(NtfyConfig {
                server_url: std::env::var("NTFY_SERVER_URL")
                    .unwrap_or_else(|_| "https://ntfy.sh".into()),
                topic,
                access_token: std::env::var("NTFY_ACCESS_TOKEN").ok(),
            });
        }

        #[cfg(feature = "notify-error-cmdline")]
        if let Ok(api_key) = std::env::var("CMDLINE_API_KEY") {
            use super::providers::CmdlineConfig;
            let mut config = CmdlineConfig::new(api_key);
            if let Ok(env) = std::env::var("ENVIRONMENT").or_else(|_| std::env::var("ENV")) {
                config = config.with_environment(env);
            }
            if let Ok(service) = std::env::var("SERVICE").or_else(|_| std::env::var("APP_NAME")) {
                config = config.with_service(service);
            }
            if let Ok(release) = std::env::var("RELEASE").or_else(|_| std::env::var("VERSION")) {
                config = config.with_release(release);
            }
            builder = builder.with_cmdline(config);
        }

        builder.build()
    }

    /// Check if any providers are configured.
    pub fn has_providers(&self) -> bool {
        !self.providers.is_empty()
    }

    /// Get the number of configured providers.
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Send a notification to all configured providers.
    ///
    /// This method sends the event to all providers concurrently and logs any failures.
    /// It does not fail if individual providers fail - errors are logged and execution continues.
    pub async fn notify(&self, event: &ErrorEvent) {
        if self.providers.is_empty() {
            return;
        }

        let futures: Vec<_> = self
            .providers
            .iter()
            .map(|provider| {
                let client = self.client.clone();
                let provider = Arc::clone(provider);
                let event = event.clone();
                async move {
                    let name = provider.name();
                    if let Err(err) = provider.notify(&client, &event).await {
                        tracing::warn!(provider = name, error = %err, "notification failed");
                    }
                }
            })
            .collect();

        futures::future::join_all(futures).await;
    }
}

impl Default for NotificationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for configuring a [`NotificationManager`].
pub struct NotificationManagerBuilder {
    providers: Vec<Arc<dyn ErrorNotifier>>,
}

impl NotificationManagerBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Add a custom provider.
    pub fn with_provider(mut self, provider: Arc<dyn ErrorNotifier>) -> Self {
        self.providers.push(provider);
        self
    }

    /// Add a Slack provider with the given configuration.
    #[cfg(feature = "notify-error-slack")]
    pub fn with_slack(self, config: super::providers::SlackConfig) -> Self {
        self.with_provider(Arc::new(SlackProvider::new(config)))
    }

    /// Add a Discord provider with the given configuration.
    #[cfg(feature = "notify-error-discord")]
    pub fn with_discord(self, config: super::providers::DiscordConfig) -> Self {
        self.with_provider(Arc::new(DiscordProvider::new(config)))
    }

    /// Add an ntfy provider with the given configuration.
    #[cfg(feature = "notify-error-ntfy")]
    pub fn with_ntfy(self, config: super::providers::NtfyConfig) -> Self {
        self.with_provider(Arc::new(NtfyProvider::new(config)))
    }

    /// Add a cmdline.io provider with the given configuration.
    #[cfg(feature = "notify-error-cmdline")]
    pub fn with_cmdline(self, config: super::providers::CmdlineConfig) -> Self {
        self.with_provider(Arc::new(CmdlineProvider::new(config)))
    }

    /// Build the notification manager.
    pub fn build(self) -> NotificationManager {
        NotificationManager {
            providers: self.providers,
            client: Client::new(),
        }
    }
}

impl Default for NotificationManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}
