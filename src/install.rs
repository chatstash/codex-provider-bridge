use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::config::{load_bridge_config, save_bridge_config};
use crate::error::{err, Result};
use crate::paths::{
    get_backup_dir, get_bridge_home, get_codex_config_path, get_install_state_path,
};
use crate::toml_patch::patch_codex_config;
use crate::types::{BridgeConfig, InstallResult, RestoreResult};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallState {
    codex_config_path: PathBuf,
    backup_path: PathBuf,
    installed_at: String,
}

pub async fn install_bridge(
    bridge_home: Option<PathBuf>,
    codex_config_path: Option<PathBuf>,
    config: Option<BridgeConfig>,
) -> Result<InstallResult> {
    let bridge_home = match bridge_home {
        Some(path) => path,
        None => get_bridge_home(None)?,
    };
    let codex_config_path = match codex_config_path {
        Some(path) => path,
        None => get_codex_config_path(None)?,
    };
    let bridge_config_path = get_bridge_config_path_for_home(&bridge_home);
    let config = match config {
        Some(config) => config,
        None => load_bridge_config(Some(&bridge_config_path)).await?,
    };
    let backup_dir = get_backup_dir(&bridge_home);
    let backup_path = backup_dir.join(format!("config.toml.{}.bak", timestamp()));

    if let Some(parent) = codex_config_path.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::create_dir_all(&backup_dir).await?;

    let current = match fs::read_to_string(&codex_config_path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(err(format!(
                "Failed to read {}: {error}",
                codex_config_path.display()
            )))
        }
    };

    fs::write(&backup_path, &current).await?;
    fs::write(&codex_config_path, patch_codex_config(&current, &config)).await?;
    save_bridge_config(&config, Some(&bridge_config_path)).await?;

    let state = InstallState {
        codex_config_path: codex_config_path.clone(),
        backup_path: backup_path.clone(),
        installed_at: Utc::now().to_rfc3339(),
    };
    fs::write(
        get_install_state_path(&bridge_home),
        format!("{}\n", serde_json::to_string_pretty(&state)?),
    )
    .await?;

    Ok(InstallResult {
        codex_config_path,
        bridge_config_path,
        backup_path,
    })
}

pub async fn restore_bridge(
    bridge_home: Option<PathBuf>,
    codex_config_path: Option<PathBuf>,
) -> Result<RestoreResult> {
    let bridge_home = match bridge_home {
        Some(path) => path,
        None => get_bridge_home(None)?,
    };
    let state_path = get_install_state_path(&bridge_home);
    let state: InstallState = serde_json::from_str(&fs::read_to_string(&state_path).await?)
        .map_err(|error| {
            err(format!(
                "Invalid install state in {}: {error}",
                state_path.display()
            ))
        })?;
    let codex_config_path = codex_config_path.unwrap_or(state.codex_config_path);
    let backup = fs::read_to_string(&state.backup_path).await?;
    if let Some(parent) = codex_config_path.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(&codex_config_path, backup).await?;
    Ok(RestoreResult {
        codex_config_path,
        backup_path: state.backup_path,
    })
}

fn timestamp() -> String {
    Utc::now().to_rfc3339().replace([':', '.'], "-")
}

fn get_bridge_config_path_for_home(bridge_home: &Path) -> PathBuf {
    bridge_home.join("config.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::default_config;

    #[tokio::test]
    async fn install_backs_up_and_restores_config() {
        let temp = std::env::temp_dir().join(format!("cpb-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let bridge_home = temp.join("bridge");
        let codex_config = temp.join("codex/config.toml");
        std::fs::create_dir_all(codex_config.parent().unwrap()).unwrap();
        fs::write(&codex_config, "model_provider = \"openai\"\n")
            .await
            .unwrap();

        let result = install_bridge(
            Some(bridge_home.clone()),
            Some(codex_config.clone()),
            Some(default_config()),
        )
        .await
        .unwrap();
        assert!(result.backup_path.exists());
        assert!(fs::read_to_string(&codex_config)
            .await
            .unwrap()
            .contains("codex_provider_bridge"));

        restore_bridge(Some(bridge_home), Some(codex_config.clone()))
            .await
            .unwrap();
        assert_eq!(
            fs::read_to_string(&codex_config).await.unwrap(),
            "model_provider = \"openai\"\n"
        );
        let _ = std::fs::remove_dir_all(&temp);
    }
}
