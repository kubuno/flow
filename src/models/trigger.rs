use serde::{Deserialize, Serialize};

/// Source de déclenchement d'une exécution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TriggerSource {
    Manual,
    Webhook,
    Cron,
    Event,
    Polling,
}

impl TriggerSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            TriggerSource::Manual  => "manual",
            TriggerSource::Webhook => "webhook",
            TriggerSource::Cron    => "cron",
            TriggerSource::Event   => "event",
            TriggerSource::Polling => "polling",
        }
    }
}

/// Configuration d'un nœud déclencheur, telle que stockée dans `node.config`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TriggerConfig {
    /// Pour trigger.cron : expression cron à 5 champs.
    #[serde(default)]
    pub cron: Option<String>,
    /// Pour trigger.kubuno_event : type d'événement à écouter.
    #[serde(default)]
    pub event_type: Option<String>,
}
