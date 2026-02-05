//! Discord webhook notification provider.

use reqwest::Client;

use crate::notifier::{ErrorEvent, ErrorNotifier, NotifyFuture};

/// Configuration for the Discord provider.
#[derive(Debug, Clone)]
pub struct DiscordConfig {
    /// Webhook URL for the Discord channel
    pub webhook_url: String,
    /// Optional mention to include (e.g., "@oncall", "<@123456789>", "<@&role_id>")
    pub mention: Option<String>,
}

impl DiscordConfig {
    /// Create a new config with just the webhook URL.
    pub fn new(webhook_url: impl Into<String>) -> Self {
        Self {
            webhook_url: webhook_url.into(),
            mention: None,
        }
    }

    /// Set a mention to include in notifications.
    ///
    /// Examples:
    /// - `"<@123456789>"` - mention a specific user by ID
    /// - `"<@&987654321>"` - mention a role by ID
    /// - `"@everyone"` - mention everyone (if webhook has permission)
    pub fn with_mention(mut self, mention: impl Into<String>) -> Self {
        self.mention = Some(mention.into());
        self
    }
}

/// Discord notification provider using webhooks.
///
/// Sends formatted error messages to a Discord channel via webhook.
///
/// # Example
///
/// ```rust,ignore
/// use axtra::notifier::{NotificationManager, DiscordConfig};
///
/// // Simple usage
/// let manager = NotificationManager::builder()
///     .with_discord(DiscordConfig::new("https://discord.com/api/webhooks/..."))
///     .build();
///
/// // With mention
/// let manager = NotificationManager::builder()
///     .with_discord(DiscordConfig::new("https://discord.com/api/webhooks/...")
///         .with_mention("<@&123456789>"))
///     .build();
/// ```
pub struct DiscordProvider {
    config: DiscordConfig,
}

impl DiscordProvider {
    /// Create a new Discord provider with the given configuration.
    pub fn new(config: DiscordConfig) -> Self {
        Self { config }
    }
}

impl ErrorNotifier for DiscordProvider {
    fn notify<'a>(&'a self, client: &'a Client, event: &'a ErrorEvent) -> NotifyFuture<'a> {
        Box::pin(async move {
            let formatted_message = format!("[{}] {}", event.location, event.message);

            let mut fields = vec![serde_json::json!({
                "name": "Details",
                "value": format!("```{}```", formatted_message),
                "inline": false
            })];

            if let Some(ref mention) = self.config.mention {
                fields.push(serde_json::json!({
                    "name": "\u{200B}",
                    "value": mention,
                    "inline": false
                }));
            }

            let embeds = serde_json::json!([
                {
                    "title": format!(":red_circle: {:?} — {}", event.error_code, event.app_name),
                    "color": 16711680, // Red
                    "fields": fields
                }
            ]);

            let payload = serde_json::json!({ "embeds": embeds });

            client
                .post(&self.config.webhook_url)
                .json(&payload)
                .send()
                .await?
                .error_for_status()?;

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "discord"
    }
}
