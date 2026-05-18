use std::path::Path;

use tokio::fs;

use crate::error::{err, Result};
use crate::paths::{default_config, get_bridge_config_path};
use crate::types::{BridgeConfig, PartialBridgeConfig, PublicBridgeConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedApiKey {
    pub api_key: Option<String>,
    pub source: Option<String>,
}

pub fn merge_config(raw: PartialBridgeConfig) -> BridgeConfig {
    let defaults = default_config();
    BridgeConfig {
        host: non_empty(raw.host).unwrap_or(defaults.host),
        port: raw.port.unwrap_or(defaults.port),
        upstream_base_url: non_empty(raw.upstream_base_url).unwrap_or(defaults.upstream_base_url),
        api_key_env: non_empty(raw.api_key_env).unwrap_or(defaults.api_key_env),
        api_key: raw.api_key.and_then(|value| {
            let trimmed = value.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        }),
        provider_id: non_empty(raw.provider_id).unwrap_or(defaults.provider_id),
        provider_name: non_empty(raw.provider_name).unwrap_or(defaults.provider_name),
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

pub fn assert_configured(config: &BridgeConfig) -> Result<()> {
    if config.upstream_base_url.trim().is_empty() {
        return Err(err(
            "缺少上游地址。请运行 codex-provider-bridge setup，并填写你的 OpenAI 兼容 /v1 地址。",
        ));
    }
    let lower = config.upstream_base_url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(err(
            "上游地址必须以 http:// 或 https:// 开头。请重新运行 codex-provider-bridge setup。",
        ));
    }
    Ok(())
}

pub async fn load_bridge_config(path: Option<&Path>) -> Result<BridgeConfig> {
    let path_buf;
    let path = match path {
        Some(path) => path,
        None => {
            path_buf = get_bridge_config_path(None)?;
            &path_buf
        }
    };

    match fs::read_to_string(path).await {
        Ok(raw) => {
            let parsed: BridgeConfig = serde_json::from_str(&raw).map_err(|error| {
                err(format!("Invalid bridge config {}: {error}", path.display()))
            })?;
            Ok(merge_config(PartialBridgeConfig {
                host: Some(parsed.host),
                port: Some(parsed.port),
                upstream_base_url: Some(parsed.upstream_base_url),
                api_key_env: Some(parsed.api_key_env),
                api_key: parsed.api_key,
                provider_id: Some(parsed.provider_id),
                provider_name: Some(parsed.provider_name),
            }))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(default_config()),
        Err(error) => Err(err(format!("Failed to read {}: {error}", path.display()))),
    }
}

pub async fn save_bridge_config(config: &BridgeConfig, path: Option<&Path>) -> Result<()> {
    let path_buf;
    let path = match path {
        Some(path) => path,
        None => {
            path_buf = get_bridge_config_path(None)?;
            &path_buf
        }
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let content = format!("{}\n", serde_json::to_string_pretty(config)?);
    fs::write(path, content).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn resolve_api_key(config: &BridgeConfig) -> ResolvedApiKey {
    if let Ok(value) = std::env::var(&config.api_key_env) {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return ResolvedApiKey {
                api_key: Some(trimmed),
                source: Some("environment".to_string()),
            };
        }
    }
    if let Some(value) = &config.api_key {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return ResolvedApiKey {
                api_key: Some(trimmed),
                source: Some("config".to_string()),
            };
        }
    }
    ResolvedApiKey {
        api_key: None,
        source: None,
    }
}

pub fn public_bridge_config(config: &BridgeConfig) -> PublicBridgeConfig {
    PublicBridgeConfig {
        host: config.host.clone(),
        port: config.port,
        upstream_base_url: config.upstream_base_url.clone(),
        api_key_env: config.api_key_env.clone(),
        provider_id: config.provider_id.clone(),
        provider_name: config.provider_name.clone(),
        api_key_saved: config
            .api_key
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_config_uses_defaults_and_trims_key() {
        let config = merge_config(PartialBridgeConfig {
            api_key: Some("  secret  ".to_string()),
            ..Default::default()
        });

        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 11435);
        assert_eq!(config.api_key.as_deref(), Some("secret"));
    }

    #[test]
    fn public_config_redacts_key() {
        let mut config = default_config();
        config.api_key = Some("secret".to_string());
        let public = public_bridge_config(&config);
        assert!(public.api_key_saved);
    }
}
