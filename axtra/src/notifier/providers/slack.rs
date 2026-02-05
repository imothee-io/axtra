//! Slack webhook notification provider.

use reqwest::Client;

use crate::notifier::{ErrorEvent, ErrorNotifier, NotifyFuture};

/// Configuration for the Slack provider.
#[derive(Debug, Clone)]
pub struct SlackConfig {
    /// Webhook URL for the Slack channel
    pub webhook_url: String,
    /// Optional mention to include (e.g., "@oncall", "<@U123456>", "<!channel>")
    pub mention: Option<String>,
}

impl SlackConfig {
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
    /// - `"@oncall"` - mention a user group
    /// - `"<@U123456>"` - mention a specific user by ID
    /// - `"<!channel>"` - mention the entire channel
    pub fn with_mention(mut self, mention: impl Into<String>) -> Self {
        self.mention = Some(mention.into());
        self
    }
}

/// Slack notification provider using incoming webhooks.
///
/// Sends formatted error messages to a Slack channel via webhook.
///
/// # Example
///
/// ```rust,ignore
/// use axtra::notifier::{NotificationManager, SlackConfig};
///
/// // Simple usage
/// let manager = NotificationManager::builder()
///     .with_slack(SlackConfig::new("https://hooks.slack.com/services/..."))
///     .build();
///
/// // With mention
/// let manager = NotificationManager::builder()
///     .with_slack(SlackConfig::new("https://hooks.slack.com/services/...")
///         .with_mention("@oncall"))
///     .build();
/// ```
pub struct SlackProvider {
    config: SlackConfig,
}

impl SlackProvider {
    /// Create a new Slack provider with the given configuration.
    pub fn new(config: SlackConfig) -> Self {
        Self { config }
    }
}

impl ErrorNotifier for SlackProvider {
    fn notify<'a>(&'a self, client: &'a Client, event: &'a ErrorEvent) -> NotifyFuture<'a> {
        Box::pin(async move {
            let mut blocks = vec![
                serde_json::json!({
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": format!(":red_circle: *{:?}* — `{}`", event.error_code, event.app_name)
                    }
                }),
                serde_json::json!({
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": format!("```[{}] {}```", event.location, event.message)
                    }
                }),
            ];

            if let Some(ref mention) = self.config.mention {
                blocks.push(serde_json::json!({
                    "type": "context",
                    "elements": [
                        {
                            "type": "mrkdwn",
                            "text": mention
                        }
                    ]
                }));
            }

            let payload = serde_json::json!({ "blocks": blocks });

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
        "slack"
    }
}
