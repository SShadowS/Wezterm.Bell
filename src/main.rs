use std::fs;
use std::io::Read;
use std::path::PathBuf;

fn main() {
    let signal_path: PathBuf = home_dir().join(".claude").join("bell_signal");

    // Extract project name from CLAUDE_PROJECT_DIR (last path component)
    let project_name = std::env::var("CLAUDE_PROJECT_DIR")
        .ok()
        .and_then(|dir| {
            dir.replace('\\', "/")
                .split('/')
                .rfind(|s| !s.is_empty())
                .map(String::from)
        })
        .unwrap_or_else(|| "Claude Code".to_string());

    // Read pane ID from WEZTERM_PANE
    let pane_id = std::env::var("WEZTERM_PANE").unwrap_or_default();

    // Read stdin (Claude Code pipes JSON and closes the pipe)
    let mut stdin_buf = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin_buf);

    // Extract fields from JSON
    let hook_event = extract_json_string(&stdin_buf, "hook_event_name").unwrap_or_default();
    let message = extract_json_string(&stdin_buf, "message").unwrap_or_default();

    // Skip subagent stops — only notify for the main conversation
    let agent_id = extract_json_string(&stdin_buf, "agent_id").unwrap_or_default();
    if hook_event == "SubagentStop" || !agent_id.is_empty() {
        return;
    }

    // Write signal file: project_name\npane_id\nhook_event\nmessage
    let content = format!("{}\n{}\n{}\n{}", project_name, pane_id, hook_event, message);
    let _ = fs::write(&signal_path, content);
}

/// Extract a string value for the given key from a JSON string.
/// Handles basic escape sequences (\", \\, \n, \t, \/).
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    // Search for "key"
    let needle = format!("\"{}\"", key);
    let key_pos = json.find(&needle)?;
    let after_key = &json[key_pos + needle.len()..];

    // Skip whitespace and colon
    let after_colon = after_key.trim_start();
    let after_colon = after_colon.strip_prefix(':')?;
    let after_colon = after_colon.trim_start();

    // Expect opening quote
    let after_colon = after_colon.strip_prefix('"')?;

    // Read until unescaped closing quote
    let mut result = String::new();
    let mut chars = after_colon.chars();
    loop {
        match chars.next()? {
            '\\' => match chars.next()? {
                '"' => result.push('"'),
                '\\' => result.push('\\'),
                'n' => result.push('\n'),
                't' => result.push('\t'),
                '/' => result.push('/'),
                other => {
                    result.push('\\');
                    result.push(other);
                }
            },
            '"' => break,
            c => result.push(c),
        }
    }
    Some(result)
}

fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .expect("USERPROFILE not set")
}
