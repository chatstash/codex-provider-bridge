mod config;
mod daemon;
mod doctor;
mod error;
mod install;
mod paths;
mod proxy;
mod setup;
mod startup;
mod toml_patch;
mod types;

use clap::{Parser, Subcommand};

use crate::config::{
    assert_configured, load_bridge_config, merge_config, resolve_api_key, save_bridge_config,
};
use crate::daemon::{can_connect, daemon_status, start_daemon, stop_daemon};
use crate::doctor::{doctor, format_doctor_report};
use crate::error::{err, Result};
use crate::install::{install_bridge, restore_bridge};
use crate::paths::get_bridge_config_path;
use crate::setup::setup_bridge;
use crate::startup::{format_startup_status, install_startup, startup_status, uninstall_startup};
use crate::types::BridgeConfig;

#[derive(Parser, Debug)]
#[command(name = "codex-provider-bridge", disable_help_subcommand = true)]
#[command(about = "Local OpenAI-compatible provider bridge for Codex ChatGPT auth mode.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, global = true)]
    host: Option<String>,

    #[arg(long, global = true)]
    port: Option<u16>,

    #[arg(long, global = true)]
    upstream: Option<String>,

    #[arg(long = "api-key-env", global = true)]
    api_key_env: Option<String>,

    #[arg(long = "api-key", global = true)]
    api_key: Option<String>,

    #[arg(long = "provider-id", global = true)]
    provider_id: Option<String>,

    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand, Debug, Clone, Copy)]
#[command(rename_all = "kebab-case")]
enum Command {
    Help,
    Setup,
    Start,
    Stop,
    Status,
    Serve,
    InstallStartup,
    UninstallStartup,
    StartupStatus,
    Install,
    Restore,
    Doctor,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{}", format_error(&error));
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Command::Help);

    match command {
        Command::Help => {
            print!("{}", usage());
        }
        Command::Setup => {
            let result = setup_bridge(overrides_from_cli(&cli)).await?;
            println!();
            println!("配置已保存: {}", result.bridge_config_path.display());
            println!("Codex 配置已更新: {}", result.codex_config_path.display());
            println!("备份已创建: {}", result.backup_path.display());
            println!();
            println!("下一步:");
            println!("  1. 运行 codex-provider-bridge start");
            println!("  2. 如需开机自启，运行 codex-provider-bridge install-startup");
            println!("  3. 重启 Codex");
            println!("  4. 如果有问题，运行 codex-provider-bridge doctor");
        }
        Command::Start => {
            let result = start_daemon().await?;
            if result.already_running {
                if let Some(pid) = result.pid {
                    println!("后台服务已经在运行，PID {pid}");
                }
            } else if let Some(pid) = result.pid {
                println!("后台服务已启动，PID {pid}");
            }
            println!("本地地址: {}", result.url);
            println!("状态文件: {}", result.state_path.display());
            println!("日志文件: {}", result.log_path.display());
            println!("下一步: 重启 Codex。查看状态可运行 codex-provider-bridge status。");
        }
        Command::Stop => {
            let result = stop_daemon().await?;
            if result.stopped {
                println!("后台服务已停止，PID {}", result.pid.unwrap_or_default());
            } else if let Some(pid) = result.pid {
                println!(
                    "后台服务未运行，已清理失效状态文件: {}",
                    result.state_path.display()
                );
                println!("PID {pid}");
            } else {
                println!("后台服务未运行。");
            }
        }
        Command::Status => {
            let status = daemon_status(None).await?;
            let config = config_from_saved_and_flags(&cli).await?;
            let port_open = can_connect(&config.host, config.port, 800).await;
            if status.running {
                println!("后台服务: 正在运行，PID {}", status.pid.unwrap_or_default());
            } else if status.stale {
                println!(
                    "后台服务: 未运行，状态文件已失效，PID {}",
                    status.pid.unwrap_or_default()
                );
                println!("修复: 运行 codex-provider-bridge start 重新启动。");
            } else {
                println!("后台服务: 未运行");
                println!("启动: codex-provider-bridge start");
            }
            if port_open {
                println!(
                    "本地端口: 正在监听 http://{}:{}/v1",
                    config.host, config.port
                );
            } else {
                println!("本地端口: 未监听");
            }
            println!("状态文件: {}", status.state_path.display());
            println!("日志文件: {}", status.log_path.display());
        }
        Command::InstallStartup => {
            let config = load_bridge_config(None).await?;
            assert_configured(&config)?;
            if resolve_api_key(&config).api_key.is_none() {
                return Err(err(format!(
                    "Missing API key. Run \"codex-provider-bridge setup\" or set {} before installing autostart.",
                    config.api_key_env
                )));
            }
            let result = install_startup(None).await?;
            println!("开机自启已安装。");
            println!("{}", format_startup_status(&result));
            if result.method.as_deref() == Some("systemd-user") {
                println!("Linux 提示: 默认在当前用户登录后启动；若需要未登录也启动，可启用 systemd linger。");
            }
        }
        Command::UninstallStartup => {
            let result = uninstall_startup(None).await?;
            if result.removed {
                println!("开机自启已移除。");
            } else {
                println!("开机自启未安装。");
            }
            println!("{}", format_startup_status(&result.status));
        }
        Command::StartupStatus => {
            println!("{}", format_startup_status(&startup_status(None).await?));
        }
        Command::Serve => {
            proxy::serve(config_from_saved_and_flags(&cli).await?).await?;
        }
        Command::Install => {
            let config = config_from_saved_and_flags(&cli).await?;
            assert_configured(&config)?;
            save_bridge_config(&config, Some(&get_bridge_config_path(None)?)).await?;
            let result = install_bridge(None, None, Some(config)).await?;
            println!("Codex 配置已更新: {}", result.codex_config_path.display());
            println!("备份已创建: {}", result.backup_path.display());
            println!("桥接配置: {}", result.bridge_config_path.display());
            println!("下一步: 运行 codex-provider-bridge start，然后重启 Codex。");
        }
        Command::Restore => {
            let result = restore_bridge(None, None).await?;
            println!("Codex 配置已恢复: {}", result.codex_config_path.display());
            println!("使用备份: {}", result.backup_path.display());
        }
        Command::Doctor => {
            let result = doctor().await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                print!("{}", format_doctor_report(&result));
            }
        }
    }

    Ok(())
}

fn overrides_from_cli(cli: &Cli) -> PartialBridgeConfig {
    PartialBridgeConfig {
        host: cli.host.clone(),
        port: cli.port,
        upstream_base_url: cli.upstream.clone(),
        api_key_env: cli.api_key_env.clone(),
        api_key: cli.api_key.clone(),
        provider_id: cli.provider_id.clone(),
        provider_name: None,
    }
}

async fn config_from_saved_and_flags(cli: &Cli) -> Result<BridgeConfig> {
    let saved = load_bridge_config(None).await?;
    Ok(merge_config(PartialBridgeConfig {
        host: cli.host.clone().or(Some(saved.host)),
        port: cli.port.or(Some(saved.port)),
        upstream_base_url: cli.upstream.clone().or(Some(saved.upstream_base_url)),
        api_key_env: cli.api_key_env.clone().or(Some(saved.api_key_env)),
        api_key: cli.api_key.clone().or(saved.api_key),
        provider_id: cli.provider_id.clone().or(Some(saved.provider_id)),
        provider_name: Some(saved.provider_name),
    }))
}

pub use crate::types::PartialBridgeConfig;

fn usage() -> &'static str {
    r#"codex-provider-bridge

Commands:
  setup       Run the beginner-friendly setup wizard.
  start       Start the bridge in the background.
  stop        Stop the background bridge.
  status      Show background bridge status.
  serve       Start the bridge in the foreground for debugging.
  install-startup
              Start the bridge automatically after Windows/Linux login.
  uninstall-startup
              Remove the Windows/Linux autostart entry.
  startup-status
              Show whether autostart is installed.
  install     Backup and patch ~/.codex/config.toml.
  restore     Restore ~/.codex/config.toml from the last install backup.
  doctor      Check bridge, Codex config, API key, and login status.

Options:
  --host <host>             Default: 127.0.0.1
  --port <port>             Default: 11435
  --upstream <url>          Required for first setup.
  --api-key-env <name>      Default: OPENAI_COMPAT_API_KEY
  --api-key <key>           Save API key to local bridge config.
  --provider-id <id>        Default: codex_provider_bridge
  --json                    Print machine-readable output for doctor.
"#
}

fn format_error(error: &crate::error::BoxError) -> String {
    let message = error.to_string();
    if message.contains("Address already in use") || message.contains("os error 10048") {
        return "启动失败: 本地端口已被占用。请运行 codex-provider-bridge status 查看后台状态，或运行 codex-provider-bridge setup 换一个端口。".to_string();
    }
    if message.contains("No such file") || message.contains("系统找不到指定的文件") {
        return format!("找不到需要的文件: {message}\n如果是恢复失败，请先运行 codex-provider-bridge install 或 setup 创建备份。");
    }
    format!("{message}\n需要帮助时可以运行 codex-provider-bridge doctor 查看状态。")
}
