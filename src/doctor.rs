use std::path::Path;
use std::process::Command;

use tokio::fs;

use crate::config::{load_bridge_config, public_bridge_config, resolve_api_key};
use crate::daemon::{can_connect, daemon_status};
use crate::error::Result;
use crate::paths::{get_bridge_config_path, get_bridge_home, get_codex_config_path};
use crate::startup::startup_status;
use crate::toml_patch::{has_bridge_provider, top_level_model_provider};
use crate::types::DoctorResult;

pub async fn doctor() -> Result<DoctorResult> {
    let bridge_home = get_bridge_home(None)?;
    let bridge_config_path = get_bridge_config_path(None)?;
    let codex_config_path = get_codex_config_path(None)?;
    let config = load_bridge_config(Some(&bridge_config_path)).await?;
    let key = resolve_api_key(&config);
    let codex_config_exists = path_exists(&codex_config_path).await;
    let codex_config = if codex_config_exists {
        fs::read_to_string(&codex_config_path)
            .await
            .unwrap_or_default()
    } else {
        String::new()
    };

    Ok(DoctorResult {
        bridge_config_path,
        codex_config_path,
        config: public_bridge_config(&config),
        api_key_present: key.api_key.is_some(),
        api_key_source: key.source,
        port_open: can_connect(&config.host, config.port, 800).await,
        daemon: daemon_status(Some(bridge_home.clone())).await?,
        startup: startup_status(Some(bridge_home)).await?,
        codex_config_exists,
        bridge_provider_configured: has_bridge_provider(&codex_config, &config.provider_id),
        model_provider_is_bridge: top_level_model_provider(&codex_config).as_deref()
            == Some(&config.provider_id),
        login_status: read_login_status(),
    })
}

pub fn format_doctor_report(result: &DoctorResult) -> String {
    let lines = vec![
        "codex-provider-bridge 体检结果".to_string(),
        String::new(),
        format!(
            "{} 桥接配置: {}",
            mark(true),
            result.bridge_config_path.display()
        ),
        format!(
            "{} Codex 配置: {}",
            mark(result.codex_config_exists),
            result.codex_config_path.display()
        ),
        format!(
            "{} API Key: {}",
            mark(result.api_key_present),
            if result.api_key_present {
                if result.api_key_source.as_deref() == Some("environment") {
                    format!("来自环境变量 {}", result.config.api_key_env)
                } else {
                    "已保存到本机配置".to_string()
                }
            } else {
                format!(
                    "未找到，请运行 codex-provider-bridge setup 或设置 {}",
                    result.config.api_key_env
                )
            }
        ),
        format!(
            "{} Codex provider: {}",
            mark(result.bridge_provider_configured),
            if result.bridge_provider_configured {
                format!("已写入 {}", result.config.provider_id)
            } else {
                "还没有写入桥接 provider".to_string()
            }
        ),
        format!(
            "{} 当前模型 provider: {}",
            mark(result.model_provider_is_bridge),
            if result.model_provider_is_bridge {
                result.config.provider_id.clone()
            } else {
                "Codex 尚未切到桥接 provider".to_string()
            }
        ),
        format!(
            "{} 后台进程: {}",
            mark(result.daemon.running),
            if result.daemon.running {
                format!("正在运行，PID {}", result.daemon.pid.unwrap_or_default())
            } else if result.daemon.stale {
                "状态文件已失效，可运行 codex-provider-bridge start 重新启动".to_string()
            } else {
                "未运行，请运行 codex-provider-bridge start".to_string()
            }
        ),
        format!(
            "{} 本地服务: {}",
            mark(result.port_open),
            if result.port_open {
                format!(
                    "正在监听 http://{}:{}/v1",
                    result.config.host, result.config.port
                )
            } else {
                "未监听，请运行 codex-provider-bridge start".to_string()
            }
        ),
        format!(
            "{} 日志文件: {}",
            mark(true),
            result.daemon.log_path.display()
        ),
        format!(
            "{} 开机自启: {}",
            mark(true),
            if !result.startup.supported {
                "当前系统暂不支持".to_string()
            } else if result.startup.installed {
                format!(
                    "已安装 ({})",
                    result.startup.method.as_deref().unwrap_or("unknown")
                )
            } else {
                "未安装，可运行 codex-provider-bridge install-startup".to_string()
            }
        ),
        format!(
            "{} Codex 登录: {}",
            mark(result.login_status.is_some()),
            result
                .login_status
                .as_deref()
                .unwrap_or("未检测到，请先在 Codex 中登录 ChatGPT")
        ),
        String::new(),
        if result.api_key_present
            && result.bridge_provider_configured
            && result.model_provider_is_bridge
            && result.daemon.running
            && result.port_open
        {
            "下一步: 重启 Codex。如果需要调试日志，运行 codex-provider-bridge status 查看日志位置。"
                .to_string()
        } else {
            "下一步: 运行 codex-provider-bridge setup，根据提示完成配置。".to_string()
        },
    ];
    format!("{}\n", lines.join("\n"))
}

fn mark(ok: bool) -> &'static str {
    if ok {
        "[OK]"
    } else {
        "[需要处理]"
    }
}

fn read_login_status() -> Option<String> {
    let mut candidates = Vec::new();
    if cfg!(windows) {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            candidates.push(format!("{local_app_data}\\OpenAI\\Codex\\bin\\codex.exe"));
        }
    }
    candidates.push("codex".to_string());

    for candidate in candidates {
        if let Ok(output) = Command::new(candidate).args(["login", "status"]).output() {
            if output.status.success() {
                let text = format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )
                .trim()
                .to_string();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

async fn path_exists(path: &Path) -> bool {
    fs::metadata(path).await.is_ok()
}

#[cfg(test)]
mod tests {
    use crate::paths::default_config;

    #[test]
    fn doctor_json_redacts_key() {
        let mut config = default_config();
        config.api_key = Some("secret".to_string());
        let public = crate::config::public_bridge_config(&config);
        let json = serde_json::to_string(&public).unwrap();
        assert!(!json.contains("secret"));
        assert!(json.contains("apiKeySaved"));
    }
}
