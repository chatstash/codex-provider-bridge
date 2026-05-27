use std::path::Path;
use std::process::Command;
use std::time::Duration;

use reqwest::Client;
use tokio::fs;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite};

use crate::config::{load_bridge_config, public_bridge_config, resolve_api_key};
use crate::daemon::{can_connect, daemon_status};
use crate::error::Result;
use crate::paths::{get_bridge_config_path, get_bridge_home, get_codex_config_path};
use crate::proxy::to_upstream_url;
use crate::startup::startup_status;
use crate::toml_patch::{has_bridge_provider, section_boolean, top_level_model_provider};
use crate::types::BridgeConfig;
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
    let provider_section = format!("model_providers.{}", config.provider_id);
    let bridge_provider_supports_websockets =
        section_boolean(&codex_config, &provider_section, "supports_websockets");
    let port_open = can_connect(&config.host, config.port, 800).await;
    let (websocket_probe_ok, websocket_probe_detail) =
        probe_websocket_bridge(&config, port_open).await;
    let (upstream_probe_ok, upstream_probe_status, upstream_probe_detail) =
        probe_upstream(&config, key.api_key.as_deref()).await;

    Ok(DoctorResult {
        bridge_config_path,
        codex_config_path,
        config: public_bridge_config(&config),
        api_key_present: key.api_key.is_some(),
        api_key_source: key.source,
        port_open,
        daemon: daemon_status(Some(bridge_home.clone())).await?,
        startup: startup_status(Some(bridge_home)).await?,
        codex_config_exists,
        bridge_provider_configured: has_bridge_provider(&codex_config, &config.provider_id),
        model_provider_is_bridge: top_level_model_provider(&codex_config).as_deref()
            == Some(&config.provider_id),
        bridge_provider_supports_websockets,
        bridge_provider_websocket_compatible: bridge_provider_supports_websockets == Some(true)
            && websocket_probe_ok.unwrap_or(true),
        websocket_probe_ok,
        websocket_probe_detail,
        upstream_probe_ok,
        upstream_probe_status,
        upstream_probe_detail,
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
            "{} WebSocket 配置: {}",
            mark(result.bridge_provider_supports_websockets == Some(true)),
            match (
                result.bridge_provider_supports_websockets,
                result.websocket_probe_ok,
            ) {
                (Some(true), Some(false)) => {
                    "provider 已显式启用 websocket，但运行时 Upgrade 探活失败。看下方 WebSocket 探活。".to_string()
                }
                (Some(true), _) => "已显式启用 websocket，Codex 可走 Upgrade 链路".to_string(),
                (Some(false), _) => "当前 provider 配置仍为 supports_websockets=false，但 bridge 已支持 websocket。运行 codex-provider-bridge install 修复。".to_string(),
                (None, _) => "当前 provider 未显式写入 supports_websockets=true。运行 codex-provider-bridge install 修复。".to_string(),
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
            "{} 上游探活: {}",
            mark(result.upstream_probe_ok),
            result
                .upstream_probe_detail
                .clone()
                .unwrap_or_else(|| "未执行".to_string())
        ),
        format!(
            "{} WebSocket 探活: {}",
            mark(result.websocket_probe_ok.unwrap_or(false)),
            result
                .websocket_probe_detail
                .clone()
                .unwrap_or_else(|| "未执行".to_string())
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
        next_step(result),
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

fn next_step(result: &DoctorResult) -> String {
    if !result.api_key_present
        || !result.bridge_provider_configured
        || !result.model_provider_is_bridge
    {
        return "下一步: 运行 codex-provider-bridge setup，根据提示完成配置。".to_string();
    }
    if result.bridge_provider_supports_websockets != Some(true) {
        return "下一步: 运行 codex-provider-bridge install 修复 provider 配置，然后重启 Codex。"
            .to_string();
    }
    if !result.daemon.running || !result.port_open {
        return "下一步: 运行 codex-provider-bridge start 启动本地桥接服务。".to_string();
    }
    if result.websocket_probe_ok == Some(false) {
        return "下一步: 检查 bridge 日志、上游 websocket 支持和 API Key 鉴权。".to_string();
    }
    if !result.upstream_probe_ok {
        return "下一步: 检查上游服务可用性、网关超时和 API Key 权限。".to_string();
    }
    "下一步: 重启 Codex。如果需要调试日志，运行 codex-provider-bridge status 查看日志位置。"
        .to_string()
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

async fn probe_upstream(
    config: &BridgeConfig,
    api_key: Option<&str>,
) -> (bool, Option<u16>, Option<String>) {
    let Some(api_key) = api_key else {
        return (false, None, Some("跳过：未找到 API Key。".to_string()));
    };
    if config.upstream_base_url.trim().is_empty() {
        return (false, None, Some("跳过：未配置上游地址。".to_string()));
    }

    let target = match to_upstream_url(
        "/v1/models?client_version=doctor",
        &config.upstream_base_url,
    ) {
        Ok(target) => target,
        Err(error) => {
            return (false, None, Some(format!("构造上游探活地址失败: {error}")));
        }
    };
    let client = match Client::builder()
        .http1_only()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(error) => return (false, None, Some(format!("创建探活客户端失败: {error}"))),
    };

    match client.get(&target).bearer_auth(api_key).send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            (
                status == 200,
                Some(status),
                Some(format!("GET /models -> {status}")),
            )
        }
        Err(error) => (false, None, Some(format!("GET /models 失败: {error}"))),
    }
}

async fn probe_websocket_bridge(
    config: &BridgeConfig,
    port_open: bool,
) -> (Option<bool>, Option<String>) {
    if !port_open {
        return (None, Some("跳过：本地服务未监听。".to_string()));
    }

    let host = if config.host.contains(':') && !config.host.starts_with('[') {
        format!("[{}]", config.host)
    } else {
        config.host.clone()
    };
    let target = format!(
        "ws://{host}:{}/v1/responses?client_version=doctor",
        config.port
    );

    match timeout(Duration::from_secs(10), connect_async(&target)).await {
        Ok(Ok((socket, _))) => {
            drop(socket);
            (
                Some(true),
                Some("GET /responses Upgrade -> 101".to_string()),
            )
        }
        Ok(Err(tungstenite::Error::Http(response))) => {
            let status = response.status().as_u16();
            (
                Some(false),
                Some(format!("GET /responses Upgrade -> {status}")),
            )
        }
        Ok(Err(error)) => (
            Some(false),
            Some(format!("GET /responses Upgrade 失败: {error}")),
        ),
        Err(_) => (Some(false), Some("GET /responses Upgrade 超时".to_string())),
    }
}

async fn path_exists(path: &Path) -> bool {
    fs::metadata(path).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::format_doctor_report;
    use crate::paths::default_config;
    use crate::types::{DaemonStatus, DoctorResult, PublicBridgeConfig, StartupStatus};

    #[test]
    fn doctor_json_redacts_key() {
        let mut config = default_config();
        config.api_key = Some("secret".to_string());
        let public = crate::config::public_bridge_config(&config);
        let json = serde_json::to_string(&public).unwrap();
        assert!(!json.contains("secret"));
        assert!(json.contains("apiKeySaved"));
    }

    #[test]
    fn websocket_health_is_true_when_provider_supports_it() {
        let config = default_config();
        let result = DoctorResult {
            bridge_config_path: "bridge.json".into(),
            codex_config_path: "config.toml".into(),
            config: PublicBridgeConfig {
                host: config.host,
                port: config.port,
                upstream_base_url: "https://api.example.com/v1".to_string(),
                api_key_env: config.api_key_env,
                provider_id: config.provider_id,
                provider_name: config.provider_name,
                api_key_saved: true,
            },
            api_key_present: true,
            api_key_source: Some("config".to_string()),
            port_open: true,
            daemon: DaemonStatus {
                state_path: "bridge.pid.json".into(),
                log_path: "bridge.log".into(),
                running: true,
                pid: Some(1),
                stale: false,
            },
            startup: StartupStatus {
                supported: true,
                installed: false,
                method: None,
                task_name: None,
                script_path: None,
                service_name: None,
                service_path: None,
                detail: None,
            },
            codex_config_exists: true,
            bridge_provider_configured: true,
            model_provider_is_bridge: true,
            bridge_provider_supports_websockets: Some(true),
            bridge_provider_websocket_compatible: true,
            websocket_probe_ok: Some(true),
            websocket_probe_detail: Some("GET /responses Upgrade -> 101".to_string()),
            upstream_probe_ok: true,
            upstream_probe_status: Some(200),
            upstream_probe_detail: Some("GET /models -> 200".to_string()),
            login_status: Some("Logged in".to_string()),
        };
        let report = format_doctor_report(&result);
        assert!(report.contains("已显式启用 websocket"));
        assert!(report.contains("[OK] WebSocket 配置"));
        assert!(report.contains("[OK] WebSocket 探活"));
    }
}
