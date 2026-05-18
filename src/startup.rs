use std::path::{Path, PathBuf};
use std::process::Command;

use tokio::fs;

use crate::error::{err, Result};
use crate::paths::{get_bridge_home, get_home_dir};
use crate::types::{StartupStatus, StartupUninstallResult};

pub const STARTUP_TASK_NAME: &str = "codex-provider-bridge";
pub const SYSTEMD_SERVICE_NAME: &str = "codex-provider-bridge.service";

pub async fn startup_status(bridge_home: Option<PathBuf>) -> Result<StartupStatus> {
    let bridge_home = bridge_home.unwrap_or(get_bridge_home(None)?);
    let script_path = get_startup_script_path(&bridge_home);
    let service_path = get_systemd_service_path()?;
    if cfg!(target_os = "linux") {
        let output = Command::new("systemctl")
            .args(["--user", "is-enabled", SYSTEMD_SERVICE_NAME])
            .output();
        let installed = output
            .as_ref()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "enabled")
            .unwrap_or(false);
        return Ok(StartupStatus {
            supported: true,
            installed,
            method: Some("systemd-user".to_string()),
            task_name: None,
            script_path: None,
            service_name: Some(SYSTEMD_SERVICE_NAME.to_string()),
            service_path: Some(service_path),
            detail: output.ok().map(|output| {
                format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )
                .trim()
                .to_string()
            }),
        });
    }
    if cfg!(windows) {
        let output = Command::new("schtasks.exe")
            .args(["/Query", "/TN", STARTUP_TASK_NAME, "/FO", "LIST", "/V"])
            .output();
        return Ok(StartupStatus {
            supported: true,
            installed: output
                .as_ref()
                .map(|output| output.status.success())
                .unwrap_or(false),
            method: Some("windows-task-scheduler".to_string()),
            task_name: Some(STARTUP_TASK_NAME.to_string()),
            script_path: Some(script_path),
            service_name: None,
            service_path: None,
            detail: output.ok().map(|output| {
                format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )
                .trim()
                .to_string()
            }),
        });
    }
    Ok(StartupStatus {
        supported: false,
        installed: false,
        method: None,
        task_name: Some(STARTUP_TASK_NAME.to_string()),
        script_path: Some(script_path),
        service_name: Some(SYSTEMD_SERVICE_NAME.to_string()),
        service_path: Some(service_path),
        detail: None,
    })
}

pub async fn install_startup(bridge_home: Option<PathBuf>) -> Result<StartupStatus> {
    let bridge_home = bridge_home.unwrap_or(get_bridge_home(None)?);
    let exe = std::env::current_exe()?;
    if cfg!(target_os = "linux") {
        let service_path = get_systemd_service_path()?;
        if let Some(parent) = service_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&service_path, systemd_service(&bridge_home, &exe)).await?;
        run_systemctl(&["daemon-reload"])?;
        run_systemctl(&["enable", SYSTEMD_SERVICE_NAME])?;
        return Ok(StartupStatus {
            supported: true,
            installed: true,
            method: Some("systemd-user".to_string()),
            task_name: None,
            script_path: None,
            service_name: Some(SYSTEMD_SERVICE_NAME.to_string()),
            service_path: Some(service_path),
            detail: None,
        });
    }
    if cfg!(windows) {
        let script_path = get_startup_script_path(&bridge_home);
        fs::create_dir_all(&bridge_home).await?;
        fs::write(&script_path, startup_script(&bridge_home, &exe)).await?;
        let status = Command::new("schtasks.exe")
            .args([
                "/Create",
                "/TN",
                STARTUP_TASK_NAME,
                "/SC",
                "ONLOGON",
                "/TR",
                &task_action(&script_path),
                "/F",
            ])
            .status()?;
        if !status.success() {
            return Err(err("创建 Windows 任务计划程序项失败。"));
        }
        return Ok(StartupStatus {
            supported: true,
            installed: true,
            method: Some("windows-task-scheduler".to_string()),
            task_name: Some(STARTUP_TASK_NAME.to_string()),
            script_path: Some(script_path),
            service_name: None,
            service_path: None,
            detail: None,
        });
    }
    Err(err(
        "开机自启目前支持 Windows 任务计划程序和 Linux systemd user service。",
    ))
}

pub async fn uninstall_startup(bridge_home: Option<PathBuf>) -> Result<StartupUninstallResult> {
    let status = startup_status(bridge_home).await?;
    if cfg!(target_os = "linux") {
        let service_path = get_systemd_service_path()?;
        let service_exists = service_path.exists();
        if !status.installed && !service_exists {
            return Ok(StartupUninstallResult {
                status,
                removed: false,
            });
        }
        if status.installed {
            run_systemctl(&["disable", SYSTEMD_SERVICE_NAME])?;
        }
        let _ = fs::remove_file(&service_path).await;
        let _ = run_systemctl(&["daemon-reload"]);
        let mut next = status;
        next.installed = false;
        return Ok(StartupUninstallResult {
            status: next,
            removed: true,
        });
    }
    if cfg!(windows) {
        if !status.installed {
            return Ok(StartupUninstallResult {
                status,
                removed: false,
            });
        }
        let result = Command::new("schtasks.exe")
            .args(["/Delete", "/TN", STARTUP_TASK_NAME, "/F"])
            .status()?;
        let mut next = status;
        next.installed = false;
        return Ok(StartupUninstallResult {
            status: next,
            removed: result.success(),
        });
    }
    Err(err(
        "开机自启目前支持 Windows 任务计划程序和 Linux systemd user service。",
    ))
}

pub fn format_startup_status(status: &StartupStatus) -> String {
    if !status.supported {
        return "开机自启: 当前系统暂不支持。".to_string();
    }
    let target = if status.method.as_deref() == Some("systemd-user") {
        format!(
            "{} ({})",
            status
                .service_name
                .as_deref()
                .unwrap_or(SYSTEMD_SERVICE_NAME),
            status
                .service_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        )
    } else {
        format!(
            "{} ({})",
            status.task_name.as_deref().unwrap_or(STARTUP_TASK_NAME),
            status
                .script_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        )
    };
    format!(
        "开机自启: {}\n方式: {}\n目标: {}",
        if status.installed {
            "已安装"
        } else {
            "未安装"
        },
        status.method.as_deref().unwrap_or("unknown"),
        target
    )
}

pub fn get_startup_script_path(bridge_home: &Path) -> PathBuf {
    bridge_home.join("startup.cmd")
}

fn get_systemd_service_path() -> Result<PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or(get_home_dir(None)?.join(".config"));
    Ok(base.join("systemd/user").join(SYSTEMD_SERVICE_NAME))
}

fn startup_script(bridge_home: &Path, exe: &Path) -> String {
    format!(
        "@echo off\r\nset \"CODEX_PROVIDER_BRIDGE_HOME={}\"\r\n{} start\r\n",
        escape_batch_value(&bridge_home.display().to_string()),
        quote_batch_arg(&exe.display().to_string())
    )
}

fn task_action(script_path: &Path) -> String {
    format!(
        "\"{}\"",
        script_path.display().to_string().replace('"', "\"\"")
    )
}

fn systemd_service(bridge_home: &Path, exe: &Path) -> String {
    format!(
        "[Unit]\nDescription=codex-provider-bridge autostart\nAfter=default.target\n\n[Service]\nType=oneshot\nRemainAfterExit=yes\nEnvironment={}\nExecStart={} start\nExecStop={} stop\n\n[Install]\nWantedBy=default.target\n",
        quote_systemd_value(&format!("CODEX_PROVIDER_BRIDGE_HOME={}", bridge_home.display())),
        quote_systemd_value(&exe.display().to_string()),
        quote_systemd_value(&exe.display().to_string()),
    )
}

fn quote_batch_arg(value: &str) -> String {
    format!("\"{}\"", escape_batch_value(value).replace('"', "\"\""))
}

fn escape_batch_value(value: &str) -> String {
    value.replace('%', "%%")
}

fn quote_systemd_value(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
            .replace('$', "$$")
    )
}

fn run_systemctl(args: &[&str]) -> Result<()> {
    let status = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()?;
    if !status.success() {
        return Err(err(format!("systemctl --user {} failed", args.join(" "))));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_windows_startup_script() {
        let script = startup_script(
            Path::new("C:\\Users\\me\\.codex-provider-bridge"),
            Path::new("C:\\bin\\codex-provider-bridge.exe"),
        );
        assert!(script.contains("CODEX_PROVIDER_BRIDGE_HOME=C:\\Users\\me\\.codex-provider-bridge"));
        assert!(script.contains("\"C:\\bin\\codex-provider-bridge.exe\" start"));
    }

    #[test]
    fn creates_linux_systemd_service() {
        let service = systemd_service(
            Path::new("/home/me/.codex-provider-bridge"),
            Path::new("/usr/bin/codex-provider-bridge"),
        );
        assert!(service.contains("ExecStart=\"/usr/bin/codex-provider-bridge\" start"));
        assert!(service.contains("ExecStop=\"/usr/bin/codex-provider-bridge\" stop"));
    }
}
