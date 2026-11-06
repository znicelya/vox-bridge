use serde::{Deserialize, Serialize};

use crate::audio::platform::inventory::RuntimeEndpoint;

use super::{
    session::{ProviderId, SessionConfig, SessionDirection},
    transcript::RuntimeFailureCode,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum DeviceSelection {
    Live {
        input_id: String,
        m1_output_id: String,
    },
    Call {
        a_input_id: String,
        m2_input_id: String,
        m1_output_id: String,
        physical_output_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLaunch {
    pub config: SessionConfig,
    pub provider_config_id: uuid::Uuid,
    pub devices: DeviceSelection,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionContext {
    pub launch: SessionLaunch,
    pub provider: ProviderId,
    pub provider_name: String,
    pub endpoints: Vec<RuntimeEndpoint>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectionPhase {
    Starting,
    Running,
    Reconnecting,
    Paused,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectionSnapshot {
    pub direction: SessionDirection,
    pub phase: DirectionPhase,
    pub stream_epoch: u64,
    pub failure: Option<RuntimeFailureCode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePhase {
    Starting,
    Running,
    Paused,
    Stopping,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RuntimeOutcome {
    Completed,
    Failed { codes: Vec<RuntimeFailureCode> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSnapshot {
    pub phase: RuntimePhase,
    pub directions: Vec<DirectionSnapshot>,
    pub outcome: Option<RuntimeOutcome>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::{LanguagePair, SessionDirection};

    #[test]
    fn launch_and_runtime_outcome_have_stable_tagged_json_contracts() {
        let launch = SessionLaunch {
            config: SessionConfig::live(
                LanguagePair::new("en", "zh").unwrap(),
                SessionDirection::TargetToSource,
            )
            .unwrap(),
            provider_config_id: uuid::Uuid::nil(),
            devices: DeviceSelection::Live {
                input_id: "capture".into(),
                m1_output_id: "m1".into(),
            },
        };
        let value = serde_json::to_value(&launch).unwrap();
        assert_eq!(value["devices"]["mode"], "live");
        assert_eq!(value["devices"]["input_id"], "capture");
        assert_eq!(value["config"]["direction"], "target_to_source");
        assert_eq!(
            serde_json::from_value::<SessionLaunch>(value).unwrap(),
            launch
        );

        let completed = serde_json::to_value(RuntimeOutcome::Completed).unwrap();
        assert_eq!(completed, serde_json::json!({"status":"completed"}));
        let failed = RuntimeOutcome::Failed {
            codes: vec![
                RuntimeFailureCode::Provider,
                RuntimeFailureCode::CleanupTimeout,
            ],
        };
        assert_eq!(
            serde_json::to_value(&failed).unwrap(),
            serde_json::json!({"status":"failed","codes":["provider","cleanup_timeout"]})
        );
        assert_eq!(
            serde_json::from_value::<RuntimeOutcome>(serde_json::to_value(&failed).unwrap())
                .unwrap(),
            failed
        );
    }

    #[test]
    fn phases_serialize_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&DirectionPhase::Reconnecting).unwrap(),
            "\"reconnecting\""
        );
        assert_eq!(
            serde_json::to_string(&RuntimePhase::Stopping).unwrap(),
            "\"stopping\""
        );
    }
}
