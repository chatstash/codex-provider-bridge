use std::fs::OpenOptions;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use chrono::Utc;
use tokio::fs;
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::config::{assert_configured, load_bridge_config, resolve_api_key};
use crate::error::{err, Result};
use crate::paths::{
    get_bridge_config_path, get_bridge_home, get_daemon_log_path, get_daemon_state_path,
};
use crate::types::{DaemonState, DaemonStatus, StartDaemonResult, StopDaemonResult};

pub async fn can_connect(host: &str, port: u16, timeout_ms: u64) -> bool {
    let target = format!("{host}:{port}");
    matches!(
        timeout(
            Duration::from_millis(timeout_ms),
            TcpStream::connect(target)
        )
        .await,
        Ok(Ok(_))
    )
}

pub async fn daemon_status(bridge_home: Option<PathBuf>) -> Result<DaemonStatus> {
    let bridge_home = match bridge_home {
        Some(path) => path,
        None => get_bridge_home(None)?,
    };
    let state_path = get_daemon_state_path(&bridge_home);
    let default_log_path = get_daemon_log_path(&bridge_home);
    let state = read_daemon_state(&state_path).await?;
    let running = state
        .as_ref()
        .is_some_and(|state| is_process_running(state.pid));
    Ok(DaemonStatus {
        state_path,
        log_path: state
            .as_ref()
            .map(|state| state.log_path.clone())
            .unwrap_or(default_log_path),
        running,
        pid: state.as_ref().map(|state| state.pid),
        stale: state.is_some() && !running,
    })
}

pub async fn start_daemon() -> Result<StartDaemonResult> {
    let bridge_home = get_bridge_home(None)?;
    let config_path = bridge_home.join("config.json");
    let config = load_bridge_config(Some(&config_path)).await?;
    assert_configured(&config)?;
    if resolve_api_key(&config).api_key.is_none() {
        return Err(err(format!(
            "Missing API key. Run \"codex-provider-bridge setup\" or set {}.",
            config.api_key_env
        )));
    }

    let status = daemon_status(Some(bridge_home.clone())).await?;
    let url = format!("http://{}:{}/v1", config.host, config.port);
    if status.running {
        return Ok(StartDaemonResult {
            already_running: true,
            pid: status.pid,
            state_path: status.state_path,
            log_path: status.log_path,
            url,
        });
    }
    if status.stale {
        let _ = fs::remove_file(&status.state_path).await;
    }
    if can_connect(&config.host, config.port, 800).await {
        return Err(err(format!(
            "本地端口 {} 已被占用。请运行 codex-provider-bridge status 查看，或运行 setup 换一个端口。",
            config.port
        )));
    }

    fs::create_dir_all(&bridge_home).await?;
    let log_path = get_daemon_log_path(&bridge_home);
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let exe = std::env::current_exe()?;
    let mut command = Command::new(&exe);
    command
        .arg("serve")
        .env("CODEX_PROVIDER_BRIDGE_HOME", &bridge_home)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    let child = command.spawn()?;
    let pid = child.id();
    let state = DaemonState {
        pid,
        started_at: Utc::now().to_rfc3339(),
        command: format!("{} serve", exe.display()),
        log_path: log_path.clone(),
        host: config.host,
        port: config.port,
    };
    fs::write(
        &status.state_path,
        format!("{}\n", serde_json::to_string_pretty(&state)?),
    )
    .await?;
    Ok(StartDaemonResult {
        already_running: false,
        pid: Some(pid),
        state_path: status.state_path,
        log_path,
        url,
    })
}

pub async fn stop_daemon() -> Result<StopDaemonResult> {
    let status = daemon_status(None).await?;
    let Some(pid) = status.pid else {
        return Ok(StopDaemonResult {
            stopped: false,
            pid: None,
            state_path: status.state_path,
        });
    };
    if !status.running {
        let _ = fs::remove_file(&status.state_path).await;
        return Ok(StopDaemonResult {
            stopped: false,
            pid: Some(pid),
            state_path: status.state_path,
        });
    }

    terminate_process(pid);
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if !is_process_running(pid) {
            break;
        }
    }
    let _ = fs::remove_file(&status.state_path).await;
    Ok(StopDaemonResult {
        stopped: true,
        pid: Some(pid),
        state_path: status.state_path,
    })
}

async fn read_daemon_state(state_path: &std::path::Path) -> Result<Option<DaemonState>> {
    match fs::read_to_string(state_path).await {
        Ok(raw) => Ok(Some(serde_json::from_str(&raw)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn is_process_running(pid: u32) -> bool {
    #[cfg(windows)]
    {
        Command::new("cmd")
            .args([
                "/C",
                &format!("tasklist /FI \"PID eq {pid}\" | findstr /R \"\\<{pid}\\>\""),
            ])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

fn terminate_process(pid: u32) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T"])
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }
}

#[allow(dead_code)]
fn _config_path_for_bridge_home(bridge_home: &std::path::Path) -> PathBuf {
    let mut env = std::collections::HashMap::new();
    env.insert(
        "CODEX_PROVIDER_BRIDGE_HOME".to_string(),
        bridge_home.display().to_string(),
    );
    get_bridge_config_path(Some(&env)).expect("valid path")
}
