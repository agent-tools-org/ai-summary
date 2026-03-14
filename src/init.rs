// Handles Claude Code init/uninstall prompts + hooks.
// Exports: run_init() for installing/uninstalling the integration.
// Deps: serde_json, std env/fs/path/process.

use serde_json::{json, map::Map, Value};
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process;

const PROMPT: &str = r#"# ai-summary: Web Search & Fetch Proxy

`ai-summary` is installed on this system. Use it **instead of** built-in WebSearch/WebFetch tools for web research — it compresses content through a local LLM, saving 60-98% of tokens.

## When to Use

- **Any web search**: use `ai-summary "<query>"` instead of WebSearch
- **Fetching URLs**: use `ai-summary fetch <url> -p "<question>"` instead of WebFetch
- **Multiple URLs**: use `ai-summary fetch <url1> <url2> -p "<question>"`
- **GitHub code search**: use `ai-summary github "<query>"` to find code on GitHub
- **GitHub file reading**: use `ai-summary github owner/repo path/to/file` to read files from repos
- **Deep research**: add `--deep` flag for more pages (5 instead of 3)

## Commands

```bash
# Search the web (replaces WebSearch)
ai-summary "firecrawl api pricing and rate limits"

# Fetch + summarize a URL (replaces WebFetch)
ai-summary fetch https://docs.example.com/api -p "what are the key endpoints"

# Fetch multiple URLs
ai-summary fetch https://url1.com https://url2.com -p "compare these"

# Search GitHub code (requires gh CLI)
ai-summary github "extensions hook" -r badlogic/pi-mono -l typescript
ai-summary github "llm provider abstraction" -l rust

# Read a file from a GitHub repo
ai-summary github owner/repo path/to/file.ts -p "how does this work"

# Browse a repo directory
ai-summary github owner/repo src/

# Summarize a remote GitHub repo (requires repomix)
ai-summary repo user/repo -p "explain the architecture"

# Deep search (more pages)
ai-summary --deep "rust async best practices 2026"

# Raw content without summarization
ai-summary --raw fetch https://example.com
```

## GitHub Code (requires `gh` CLI)

When you need to read code from a GitHub repository, **always prefer `ai-summary github`** over:
- Fetching raw.githubusercontent.com URLs (fragile, 404-prone)
- Using WebFetch on GitHub pages (noisy HTML, wastes tokens)
- Guessing file paths in remote repos

`ai-summary github` uses the GitHub API via `gh` CLI — reliable, structured, and summarized.

### When to use which GitHub command

| Need | Command |
|------|---------|
| Find code patterns across repos | `ai-summary github "query" [-l lang]` |
| Find code in a specific repo | `ai-summary github "query" -r owner/repo` |
| Read a specific file | `ai-summary github owner/repo path/to/file` |
| Browse repo structure | `ai-summary github owner/repo [dir/]` |
| Understand entire repo | `ai-summary repo owner/repo -p "question"` |

## Rules

1. **Always prefer `ai-summary` over WebSearch/WebFetch** — it returns compressed, focused content
2. **Always prefer `ai-summary github` for GitHub code** — never fetch raw.githubusercontent.com URLs directly
3. Run via Bash tool: `ai-summary "query"` — read stdout for results
4. If `ai-summary` fails (exit code non-zero), fall back to built-in tools
5. For subagents (Agent tool): instruct them to use `ai-summary` via Bash instead of WebSearch/WebFetch
6. Do NOT use WebSearch or WebFetch directly unless `ai-summary` is unavailable or fails
"#;
const MARKER_START: &str = "<!-- ai-summary:start -->";
const MARKER_END: &str = "<!-- ai-summary:end -->";
const HOOK_SCRIPT: &str = include_str!("../hooks/prebash.sh");
const HOOK_WEBFETCH: &str = include_str!("../hooks/prewebfetch.sh");
const HOOK_WEBSEARCH: &str = include_str!("../hooks/prewebsearch.sh");
type InitResult<T> = Result<T, String>;

pub fn run_init(uninstall: bool, with_repomix: bool) {
    let result = if uninstall {
        uninstall_integration()
    } else {
        install_integration()
    };
    if let Err(err) = result {
        eprintln!("[ai-summary init] {err}");
        process::exit(1);
    }
    if uninstall {
        println!("Claude Code integration uninstalled.");
    } else {
        println!("Claude Code integration installed.");
        check_optional_deps(with_repomix);
    }
}

fn install_integration() -> InitResult<()> {
    let claude_dir = claude_dir()?;
    fs::create_dir_all(&claude_dir)
        .map_err(|e| format!("Failed to create {}: {e}", claude_dir.display()))?;
    let prompt_path = claude_dir.join("CLAUDE.md");
    install_prompt(&prompt_path)?;
    println!("Prompt installed: {}", prompt_path.display());
    let settings_path = claude_dir.join("settings.json");
    install_hooks(&settings_path)?;
    println!("Hooks installed: {}", settings_path.display());
    Ok(())
}

fn uninstall_integration() -> InitResult<()> {
    let claude_dir = claude_dir()?;
    let prompt_path = claude_dir.join("CLAUDE.md");
    remove_prompt_block(&prompt_path)?;
    println!("Prompt removed: {}", prompt_path.display());
    let settings_path = claude_dir.join("settings.json");
    remove_hook_entry(&settings_path)?;
    println!("Hook removed: {}", settings_path.display());
    for name in ["ai-summary-prebash.sh", "ai-summary-prewebfetch.sh", "ai-summary-prewebsearch.sh"] {
        let hook_file = claude_dir.join("hooks").join(name);
        if hook_file.exists() {
            let _ = fs::remove_file(&hook_file);
            println!("Hook script removed: {}", hook_file.display());
        }
    }
    Ok(())
}

fn claude_dir() -> InitResult<PathBuf> {
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .ok_or_else(|| "HOME or USERPROFILE must be set".to_string())?;
    Ok(PathBuf::from(home).join(".claude"))
}

fn install_prompt(path: &Path) -> InitResult<()> {
    let mut contents = read_or_empty(path)?;
    let block = prompt_block();
    if let Some(start) = contents.find(MARKER_START) {
        if let Some(rel_end) = contents[start..].find(MARKER_END) {
            let mut end = start + rel_end + MARKER_END.len();
            end += newline_length(&contents[end..]);
            contents.replace_range(start..end, &block);
        } else {
            contents.truncate(start);
            append_block(&mut contents, &block);
        }
    } else {
        append_block(&mut contents, &block);
    }
    fs::write(path, contents).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    Ok(())
}

fn remove_prompt_block(path: &Path) -> InitResult<()> {
    let mut contents = match fs::read_to_string(path) {
        Ok(data) => data,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(format!("Failed to read {}: {err}", path.display())),
    };
    if let Some(start) = contents.find(MARKER_START) {
        if let Some(rel_end) = contents[start..].find(MARKER_END) {
            let mut end = start + rel_end + MARKER_END.len();
            end += newline_length(&contents[end..]);
            contents.replace_range(start..end, "");
        } else {
            contents.truncate(start);
        }
        fs::write(path, contents).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    }
    Ok(())
}

fn install_hooks(settings_path: &Path) -> InitResult<()> {
    let hooks = [
        ("ai-summary-prebash.sh", HOOK_SCRIPT, "Bash"),
        ("ai-summary-prewebfetch.sh", HOOK_WEBFETCH, "WebFetch"),
        ("ai-summary-prewebsearch.sh", HOOK_WEBSEARCH, "WebSearch"),
    ];
    let mut settings = read_settings(settings_path)?;
    let map = settings
        .as_object_mut()
        .ok_or_else(|| format!("{} must be a JSON object", settings_path.display()))?;
    let hooks_entry = map.entry("hooks").or_insert_with(|| Value::Object(Map::new()));
    let hooks_obj = hooks_entry
        .as_object_mut()
        .ok_or_else(|| "hooks entry is not a JSON object".to_string())?;
    let pre_tool = hooks_obj
        .entry("PreToolUse")
        .or_insert_with(|| Value::Array(Vec::new()));
    let pre_array = pre_tool
        .as_array_mut()
        .ok_or_else(|| "PreToolUse must be an array".to_string())?;
    for (filename, content, matcher) in hooks {
        let path = write_hook_script_file(filename, content)?;
        let command = path.display().to_string();
        let already_exists = pre_array.iter().any(|entry| {
            entry.get("hooks").and_then(Value::as_array).is_some_and(|h| {
                h.iter().any(|hook| {
                    hook.get("command").and_then(Value::as_str).is_some_and(|c| c.contains(filename))
                })
            })
        });
        if !already_exists {
            pre_array.push(json!({
                "matcher": matcher,
                "hooks": [{ "type": "command", "command": command }]
            }));
        }
    }
    write_settings(settings_path, &settings)?;
    Ok(())
}

fn remove_hook_entry(settings_path: &Path) -> InitResult<()> {
    if !settings_path.exists() {
        return Ok(());
    }
    let mut settings = read_settings(settings_path)?;
    let map = settings
        .as_object_mut()
        .ok_or_else(|| format!("{} must be a JSON object", settings_path.display()))?;
    let mut changed = false;
    let mut remove_hooks = false;
    if let Some(Value::Object(hooks_obj)) = map.get_mut("hooks") {
        if let Some(Value::Array(pre)) = hooks_obj.get_mut("PreToolUse") {
            let original_len = pre.len();
            pre.retain(|entry| !hook_entry_matches(entry));
            if pre.len() != original_len {
                changed = true;
            }
            if pre.is_empty() {
                hooks_obj.remove("PreToolUse");
                changed = true;
            }
        }
        if hooks_obj.is_empty() {
            remove_hooks = true;
        }
    }
    if remove_hooks {
        map.remove("hooks");
        changed = true;
    }
    if changed {
        write_settings(settings_path, &settings)?;
    }
    Ok(())
}

fn write_hook_script_file(filename: &str, content: &str) -> InitResult<PathBuf> {
    let claude_dir = claude_dir()?;
    let hooks_dir = claude_dir.join("hooks");
    fs::create_dir_all(&hooks_dir)
        .map_err(|e| format!("Failed to create {}: {e}", hooks_dir.display()))?;
    let path = hooks_dir.join(filename);
    fs::write(&path, content)
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o755));
    }
    Ok(path)
}

fn read_or_empty(path: &Path) -> InitResult<String> {
    match fs::read_to_string(path) {
        Ok(data) => Ok(data),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(format!("Failed to read {}: {err}", path.display())),
    }
}

fn read_settings(path: &Path) -> InitResult<Value> {
    if path.exists() {
        let data = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        serde_json::from_str(&data)
            .map_err(|e| format!("Failed to parse {}: {e}", path.display()))
    } else {
        Ok(Value::Object(Map::new()))
    }
}

fn write_settings(path: &Path, value: &Value) -> InitResult<()> {
    let serialized = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize {}: {e}", path.display()))?;
    fs::write(path, format!("{serialized}\n")).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

fn hook_entry_matches(entry: &Value) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|hooks| {
            hooks.iter().any(|hook| {
                hook.as_object()
                    .and_then(|obj| obj.get("command"))
                    .and_then(Value::as_str)
                    .is_some_and(|cmd| cmd.contains("ai-summary"))
            })
        })
}

fn prompt_block() -> String {
    format!(
        "{}\n{}\n{}\n",
        MARKER_START,
        PROMPT.trim_end(),
        MARKER_END
    )
}

fn append_block(buffer: &mut String, block: &str) {
    if !buffer.is_empty() {
        if !buffer.ends_with('\n') {
            buffer.push('\n');
        }
        if !buffer.ends_with("\n\n") {
            buffer.push('\n');
        }
    }
    buffer.push_str(block);
}

fn newline_length(slice: &str) -> usize {
    if slice.starts_with("\r\n") {
        2
    } else if slice.starts_with('\n') || slice.starts_with('\r') {
        1
    } else {
        0
    }
}

fn has_command(name: &str) -> bool {
    process::Command::new(name)
        .arg("--version")
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn check_optional_deps(install_repomix: bool) {
    println!("\nOptional dependencies:");
    if has_command("repomix") {
        println!("  repomix: installed");
    } else if install_repomix {
        println!("  repomix: installing...");
        let status = process::Command::new("npm")
            .args(["install", "-g", "repomix"])
            .status();
        match status {
            Ok(s) if s.success() => println!("  repomix: installed"),
            _ => eprintln!("  repomix: install failed (run manually: npm install -g repomix)"),
        }
    } else {
        println!("  repomix: not found (optional, for `ai-summary repo` command)");
        println!("    Install: npm install -g repomix");
        println!("    Or run: ai-summary init --with-repomix");
    }
}
