use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::{err, Result};
use crate::types::BridgeConfig;

pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 11435;
pub const DEFAULT_UPSTREAM_BASE_URL: &str = "";
pub const DEFAULT_API_KEY_ENV: &str = "OPENAI_COMPAT_API_KEY";
pub const DEFAULT_PROVIDER_ID: &str = "codex_provider_bridge";
pub const DEFAULT_PROVIDER_NAME: &str = "codex-provider-bridge";

pub type EnvMap = HashMap<String, String>;

pub fn get_home_dir(env: Option<&EnvMap>) -> Result<PathBuf> {
    let home = get_env(env, "HOME").or_else(|| get_env(env, "USERPROFILE"));
    match home {
        Some(home) if !home.is_empty() => Ok(PathBuf::from(home)),
        _ => Err(err(
            "Cannot resolve home directory from HOME or USERPROFILE.",
        )),
    }
}

pub fn get_bridge_home(env: Option<&EnvMap>) -> Result<PathBuf> {
    if let Some(value) = get_env(env, "CODEX_PROVIDER_BRIDGE_HOME") {
        if !value.is_empty() {
            return Ok(PathBuf::from(value));
        }
    }
    Ok(get_home_dir(env)?.join(".codex-provider-bridge"))
}

pub fn get_bridge_config_path(env: Option<&EnvMap>) -> Result<PathBuf> {
    Ok(get_bridge_home(env)?.join("config.json"))
}

pub fn get_daemon_state_path(bridge_home: &std::path::Path) -> PathBuf {
    bridge_home.join("bridge.pid.json")
}

pub fn get_daemon_log_path(bridge_home: &std::path::Path) -> PathBuf {
    bridge_home.join("bridge.log")
}

pub fn get_install_state_path(bridge_home: &std::path::Path) -> PathBuf {
    bridge_home.join("install-state.json")
}

pub fn get_backup_dir(bridge_home: &std::path::Path) -> PathBuf {
    bridge_home.join("backups")
}

pub fn get_codex_home(env: Option<&EnvMap>) -> Result<PathBuf> {
    if let Some(value) = get_env(env, "CODEX_HOME") {
        if !value.is_empty() {
            return Ok(PathBuf::from(value));
        }
    }
    Ok(get_home_dir(env)?.join(".codex"))
}

pub fn get_codex_config_path(env: Option<&EnvMap>) -> Result<PathBuf> {
    Ok(get_codex_home(env)?.join("config.toml"))
}

pub fn default_config() -> BridgeConfig {
    BridgeConfig {
        host: DEFAULT_HOST.to_string(),
        port: DEFAULT_PORT,
        upstream_base_url: DEFAULT_UPSTREAM_BASE_URL.to_string(),
        api_key_env: DEFAULT_API_KEY_ENV.to_string(),
        api_key: None,
        provider_id: DEFAULT_PROVIDER_ID.to_string(),
        provider_name: DEFAULT_PROVIDER_NAME.to_string(),
    }
}

fn get_env(env: Option<&EnvMap>, key: &str) -> Option<String> {
    match env {
        Some(env) => env.get(key).cloned(),
        None => std::env::var(key).ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_bridge_home_from_userprofile() {
        let mut env = EnvMap::new();
        env.insert("USERPROFILE".to_string(), "C:\\Users\\me".to_string());
        assert_eq!(
            get_bridge_home(Some(&env)).unwrap(),
            PathBuf::from("C:\\Users\\me").join(".codex-provider-bridge")
        );
    }

    #[test]
    fn resolves_codex_home_from_env() {
        let mut env = EnvMap::new();
        env.insert("HOME".to_string(), "/home/me".to_string());
        env.insert("CODEX_HOME".to_string(), "/tmp/codex".to_string());
        assert_eq!(
            get_codex_config_path(Some(&env)).unwrap(),
            PathBuf::from("/tmp/codex/config.toml")
        );
    }
}
