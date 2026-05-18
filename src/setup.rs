use std::io::{self, IsTerminal, Read, Write};

use crate::config::{assert_configured, load_bridge_config, merge_config};
use crate::error::{err, Result};
use crate::install::install_bridge;
use crate::paths::{get_bridge_config_path, get_bridge_home};
use crate::types::{PartialBridgeConfig, SetupResult};

pub async fn setup_bridge(overrides: PartialBridgeConfig) -> Result<SetupResult> {
    let bridge_home = get_bridge_home(None)?;
    let bridge_config_path = get_bridge_config_path(None)?;
    let saved = load_bridge_config(Some(&bridge_config_path)).await?;
    let piped_lines = read_piped_lines()?;
    let mut input = PromptInput::new(piped_lines);

    println!();
    println!("codex-provider-bridge 安装向导");
    println!("已有配置会显示为默认值。API Key 会保存到本机配置文件，不会打印到日志。");
    println!();

    let upstream_base_url = match overrides.upstream_base_url {
        Some(value) => read_url(&value, saved.upstream_base_url.as_str())?,
        None => {
            let hint = if saved.upstream_base_url.is_empty() {
                String::new()
            } else {
                format!(" [{}]", saved.upstream_base_url)
            };
            read_url(
                &input.ask(&format!("上游 OpenAI 兼容地址{hint}: "))?,
                saved.upstream_base_url.as_str(),
            )?
        }
    };
    let port = match overrides.port {
        Some(port) => port,
        None => read_port(
            &input.ask(&format!("本地端口 [{}]: ", saved.port))?,
            saved.port,
        )?,
    };
    let api_key_env = match overrides.api_key_env {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => {
            let input_value = input.ask(&format!("环境变量名 [{}]: ", saved.api_key_env))?;
            if input_value.trim().is_empty() {
                saved.api_key_env.clone()
            } else {
                input_value.trim().to_string()
            }
        }
    };
    let api_key = match overrides.api_key {
        Some(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        _ => {
            let existing_hint = if saved.api_key.is_some() {
                "，留空则沿用已保存密钥"
            } else {
                ""
            };
            let input_value = input.ask(&format!("API Key{existing_hint}: "))?;
            let value = if !input_value.trim().is_empty() {
                Some(input_value.trim().to_string())
            } else {
                saved
                    .api_key
                    .clone()
                    .or_else(|| std::env::var(&api_key_env).ok())
            };
            if value.is_none() {
                return Err(err(format!(
                    "需要 API Key。请重新运行 setup 并填写，或先设置环境变量 {api_key_env}。"
                )));
            }
            value
        }
    };

    let config = merge_config(PartialBridgeConfig {
        host: overrides.host.or(Some(saved.host)),
        port: Some(port),
        upstream_base_url: Some(upstream_base_url),
        api_key_env: Some(api_key_env),
        api_key,
        provider_id: overrides.provider_id.or(Some(saved.provider_id)),
        provider_name: overrides.provider_name.or(Some(saved.provider_name)),
    });
    assert_configured(&config)?;

    let result = install_bridge(Some(bridge_home), None, Some(config)).await?;
    Ok(SetupResult {
        codex_config_path: result.codex_config_path,
        bridge_config_path: result.bridge_config_path,
        backup_path: result.backup_path,
    })
}

struct PromptInput {
    piped_lines: Option<Vec<String>>,
    index: usize,
}

impl PromptInput {
    fn new(piped_lines: Option<Vec<String>>) -> Self {
        Self {
            piped_lines,
            index: 0,
        }
    }

    fn ask(&mut self, question: &str) -> Result<String> {
        print!("{question}");
        io::stdout().flush()?;
        if let Some(lines) = &self.piped_lines {
            let value = lines.get(self.index).cloned().unwrap_or_default();
            self.index += 1;
            println!("{value}");
            return Ok(value.trim().to_string());
        }
        let mut value = String::new();
        io::stdin().read_line(&mut value)?;
        Ok(value.trim().to_string())
    }
}

fn read_piped_lines() -> Result<Option<Vec<String>>> {
    if io::stdin().is_terminal() {
        return Ok(None);
    }
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    Ok(Some(input.lines().map(ToString::to_string).collect()))
}

fn read_port(value: &str, fallback: u16) -> Result<u16> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(fallback);
    }
    let port: u16 = trimmed
        .parse()
        .map_err(|_| err("端口必须是 1 到 65535 之间的数字。"))?;
    if port == 0 {
        return Err(err("端口必须是 1 到 65535 之间的数字。"));
    }
    Ok(port)
}

fn read_url(value: &str, fallback: &str) -> Result<String> {
    let trimmed = value.trim();
    let selected = if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    };
    if selected.is_empty() {
        return Err(err(
            "需要上游地址。请填写你的 OpenAI 兼容 /v1 地址，例如 https://api.example.com/v1。",
        ));
    }
    let lower = selected.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(err("上游地址必须以 http:// 或 https:// 开头。"));
    }
    Ok(selected.trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_port() {
        assert_eq!(read_port("", 11435).unwrap(), 11435);
        assert_eq!(read_port("11436", 11435).unwrap(), 11436);
        assert!(read_port("0", 11435).is_err());
    }

    #[test]
    fn validates_url() {
        assert_eq!(
            read_url("https://api.example.com/v1/", "").unwrap(),
            "https://api.example.com/v1"
        );
        assert!(read_url("ftp://example.com", "").is_err());
    }
}
