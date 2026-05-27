use crate::types::BridgeConfig;

fn quote_toml(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn ensure_trailing_newline(content: &str) -> String {
    if content.ends_with('\n') {
        content.to_string()
    } else {
        format!("{content}\n")
    }
}

fn set_top_level_string(content: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = ensure_trailing_newline(content)
        .lines()
        .map(ToString::to_string)
        .collect();
    let mut in_top_level = true;
    for line in &mut lines {
        if line.trim_start().starts_with('[') {
            in_top_level = false;
        }
        if in_top_level && line.trim_start().starts_with(&format!("{key} ")) && line.contains('=') {
            *line = format!("{key} = {}", quote_toml(value));
            return format!("{}\n", lines.join("\n"));
        }
    }

    let insert_at = lines
        .iter()
        .position(|line| !line.trim_start().starts_with('#'))
        .unwrap_or(0);
    lines.insert(insert_at, format!("{key} = {}", quote_toml(value)));
    format!("{}\n", lines.join("\n"))
}

fn find_section(lines: &[String], section: &str) -> Option<(usize, usize)> {
    let header = format!("[{section}]");
    let start = lines.iter().position(|line| {
        let trimmed = line.trim();
        trimmed == header
            || trimmed
                .strip_prefix(&header)
                .is_some_and(|rest| rest.trim_start().starts_with('#'))
    })?;
    let end = lines[start + 1..]
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .map(|offset| start + 1 + offset)
        .unwrap_or(lines.len());
    Some((start, end))
}

fn ensure_section_booleans(content: &str, section: &str, values: &[(&str, bool)]) -> String {
    let mut lines: Vec<String> = ensure_trailing_newline(content)
        .lines()
        .map(ToString::to_string)
        .collect();

    if let Some((start, end)) = find_section(&lines, section) {
        let mut section_lines = lines[start..end].to_vec();
        for (key, value) in values {
            let rendered = format!("{key} = {}", if *value { "true" } else { "false" });
            if let Some(index) =
                section_lines
                    .iter()
                    .enumerate()
                    .skip(1)
                    .find_map(|(index, line)| {
                        line.trim_start()
                            .starts_with(&format!("{key} "))
                            .then_some(index)
                    })
            {
                section_lines[index] = rendered;
            } else {
                section_lines.push(rendered);
            }
        }
        lines.splice(start..end, section_lines);
        return format!("{}\n", lines.join("\n"));
    }

    if !lines.is_empty() && lines.last().is_some_and(|line| !line.trim().is_empty()) {
        lines.push(String::new());
    }
    lines.push(format!("[{section}]"));
    for (key, value) in values {
        lines.push(format!("{key} = {}", if *value { "true" } else { "false" }));
    }
    format!("{}\n", lines.join("\n"))
}

fn replace_or_append_section(content: &str, section: &str, body_lines: &[String]) -> String {
    let mut lines: Vec<String> = ensure_trailing_newline(content)
        .lines()
        .map(ToString::to_string)
        .collect();
    let replacement = std::iter::once(format!("[{section}]"))
        .chain(body_lines.iter().cloned())
        .collect::<Vec<_>>();

    if let Some((start, end)) = find_section(&lines, section) {
        lines.splice(start..end, replacement);
    } else {
        if !lines.is_empty() && lines.last().is_some_and(|line| !line.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.extend(replacement);
    }
    format!("{}\n", lines.join("\n"))
}

pub fn patch_codex_config(content: &str, config: &BridgeConfig) -> String {
    let mut next = ensure_trailing_newline(content);
    next = set_top_level_string(&next, "model_provider", &config.provider_id);
    next = ensure_section_booleans(
        &next,
        "features",
        &[("remote_control", true), ("prevent_idle_sleep", true)],
    );
    replace_or_append_section(
        &next,
        &format!("model_providers.{}", config.provider_id),
        &[
            format!("name = {}", quote_toml(&config.provider_name)),
            format!(
                "base_url = {}",
                quote_toml(&format!("http://{}:{}/v1", config.host, config.port))
            ),
            "wire_api = \"responses\"".to_string(),
            "requires_openai_auth = true".to_string(),
            "supports_websockets = true".to_string(),
        ],
    )
}

pub fn has_bridge_provider(content: &str, provider_id: &str) -> bool {
    let lines: Vec<String> = ensure_trailing_newline(content)
        .lines()
        .map(ToString::to_string)
        .collect();
    find_section(&lines, &format!("model_providers.{provider_id}")).is_some()
}

pub fn section_boolean(content: &str, section: &str, key: &str) -> Option<bool> {
    let lines: Vec<String> = ensure_trailing_newline(content)
        .lines()
        .map(ToString::to_string)
        .collect();
    let (start, end) = find_section(&lines, section)?;
    lines[start + 1..end].iter().find_map(|line| {
        let trimmed = line.trim();
        let prefix = format!("{key} ");
        let rest = trimmed.strip_prefix(&prefix)?;
        let (_, value) = rest.split_once('=')?;
        match value.trim() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    })
}

pub fn top_level_model_provider(content: &str) -> Option<String> {
    for line in ensure_trailing_newline(content).lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            return None;
        }
        if let Some(rest) = trimmed.strip_prefix("model_provider") {
            let (_, value) = rest.split_once('=')?;
            return Some(value.trim().trim_matches('"').to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::paths::default_config;

    use super::*;

    #[test]
    fn patch_adds_bridge_provider_and_features() {
        let mut config = default_config();
        config.upstream_base_url = "https://api.example.com/v1".to_string();
        let patched = patch_codex_config(
            "# keep\n[model_providers.openai]\nname = \"OpenAI\"\n",
            &config,
        );

        assert!(patched.contains("model_provider = \"codex_provider_bridge\""));
        assert!(patched.contains("[features]"));
        assert!(patched.contains("remote_control = true"));
        assert!(patched.contains("[model_providers.codex_provider_bridge]"));
        assert!(patched.contains("requires_openai_auth = true"));
        assert!(patched.contains("supports_websockets = true"));
        assert!(patched.contains("[model_providers.openai]"));
    }

    #[test]
    fn patch_is_idempotent_for_bridge_section() {
        let config = default_config();
        let once = patch_codex_config("", &config);
        let twice = patch_codex_config(&once, &config);
        assert_eq!(once, twice);
    }

    #[test]
    fn reads_section_boolean() {
        let content = r#"
[model_providers.codex_provider_bridge]
supports_websockets = true
"#;
        assert_eq!(
            section_boolean(
                content,
                "model_providers.codex_provider_bridge",
                "supports_websockets"
            ),
            Some(true)
        );
    }
}
