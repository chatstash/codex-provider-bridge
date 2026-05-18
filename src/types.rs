use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BridgeConfig {
    pub host: String,
    pub port: u16,
    pub upstream_base_url: String,
    pub api_key_env: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    pub provider_id: String,
    pub provider_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct PartialBridgeConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub upstream_base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub api_key: Option<String>,
    pub provider_id: Option<String>,
    pub provider_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PublicBridgeConfig {
    pub host: String,
    pub port: u16,
    pub upstream_base_url: String,
    pub api_key_env: String,
    pub provider_id: String,
    pub provider_name: String,
    pub api_key_saved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DaemonState {
    pub pid: u32,
    pub started_at: String,
    pub command: String,
    pub log_path: PathBuf,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DaemonStatus {
    pub state_path: PathBuf,
    pub log_path: PathBuf,
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartupStatus {
    pub supported: bool,
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StartupUninstallResult {
    pub status: StartupStatus,
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorResult {
    pub bridge_config_path: PathBuf,
    pub codex_config_path: PathBuf,
    pub config: PublicBridgeConfig,
    pub api_key_present: bool,
    pub api_key_source: Option<String>,
    pub port_open: bool,
    pub daemon: DaemonStatus,
    pub startup: StartupStatus,
    pub codex_config_exists: bool,
    pub bridge_provider_configured: bool,
    pub model_provider_is_bridge: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_status: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstallResult {
    pub codex_config_path: PathBuf,
    pub bridge_config_path: PathBuf,
    pub backup_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct RestoreResult {
    pub codex_config_path: PathBuf,
    pub backup_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SetupResult {
    pub codex_config_path: PathBuf,
    pub bridge_config_path: PathBuf,
    pub backup_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct StartDaemonResult {
    pub already_running: bool,
    pub pid: Option<u32>,
    pub state_path: PathBuf,
    pub log_path: PathBuf,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct StopDaemonResult {
    pub stopped: bool,
    pub pid: Option<u32>,
    pub state_path: PathBuf,
}
