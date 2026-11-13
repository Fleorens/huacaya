//! The JSON shapes of the OpenCode server's answers, as serde reads them; unknown keys are ignored.

use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct RawSessions {
    pub(super) data: Vec<RawSession>,
}

#[derive(Deserialize)]
pub(super) struct RawSession {
    pub(super) id: String,
    pub(super) title: String,
    #[serde(default)]
    pub(super) agent: Option<String>,
    #[serde(default)]
    pub(super) model: Option<RawModel>,
    #[serde(default)]
    pub(super) outcome: Option<String>,
    #[serde(default)]
    pub(super) tokens: Option<RawTokens>,
    #[serde(default)]
    pub(super) time: Option<RawTime>,
    #[serde(default)]
    pub(super) location: Option<RawLocation>,
}
#[derive(Deserialize)]
pub(super) struct RawModel {
    #[serde(default)]
    pub(super) id: Option<String>,
}
#[derive(Deserialize, Default)]
pub(super) struct RawTokens {
    #[serde(default)]
    pub(super) input: u64,
    #[serde(default)]
    pub(super) output: u64,
    #[serde(default)]
    pub(super) cache: RawCache,
}
#[derive(Deserialize, Default)]
pub(super) struct RawCache {
    #[serde(default)]
    pub(super) read: u64,
}
#[derive(Deserialize)]
pub(super) struct RawTime {
    #[serde(default)]
    pub(super) created: i64,
    #[serde(default)]
    pub(super) completed: Option<i64>,
    #[serde(default)]
    pub(super) idle: Option<i64>,
}
#[derive(Deserialize)]
pub(super) struct RawLocation {
    #[serde(default)]
    pub(super) directory: Option<String>,
}
#[derive(Deserialize)]
pub(super) struct RawMessages {
    pub(super) data: Vec<RawMessage>,
}
#[derive(Deserialize)]
pub(super) struct RawMessage {
    #[serde(rename = "type", default)]
    pub(super) r#type: Option<String>,
    #[serde(default)]
    pub(super) id: Option<String>,
    #[serde(default)]
    pub(super) time: Option<RawTime>,
    #[serde(default)]
    pub(super) content: Vec<RawPart>,
}
#[derive(Deserialize)]
pub(super) struct RawPart {
    #[serde(rename = "type", default)]
    pub(super) r#type: Option<String>,
    #[serde(default)]
    pub(super) id: Option<String>,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) text: Option<String>,
    #[serde(default)]
    pub(super) time: Option<RawTime>,
    #[serde(default)]
    pub(super) state: Option<RawPartState>,
}
#[derive(Deserialize)]
pub(super) struct RawPartState {
    #[serde(default)]
    pub(super) status: Option<String>,
    #[serde(default)]
    pub(super) input: Option<RawToolInput>,
    #[serde(default)]
    pub(super) content: Option<Vec<RawPartContent>>,
    #[serde(default)]
    pub(super) metadata: Option<RawMetadata>,
}
#[derive(Deserialize)]
pub(super) struct RawToolInput {
    #[serde(default)]
    pub(super) path: Option<String>,
    #[serde(default)]
    pub(super) command: Option<String>,
    #[serde(default)]
    pub(super) pattern: Option<String>,
    #[serde(default)]
    pub(super) content: Option<serde_json::Value>,
}
#[derive(Deserialize)]
pub(super) struct RawPartContent {
    #[serde(default)]
    pub(super) text: Option<String>,
}
#[derive(Deserialize)]
pub(super) struct RawMetadata {
    #[serde(default)]
    pub(super) exit: Option<i64>,
    #[serde(default)]
    pub(super) files: Option<Vec<RawFile>>,
}
#[derive(Deserialize)]
pub(super) struct RawFile {
    #[serde(default)]
    pub(super) patch: Option<String>,
}
