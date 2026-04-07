# Wezterm.Bell

![Language](https://img.shields.io/badge/language-Rust-orange?logo=rust)
![Dependencies](https://img.shields.io/badge/dependencies-zero-brightgreen)
![License](https://img.shields.io/badge/license-MIT-blue)
![Platform](https://img.shields.io/badge/platform-Windows-lightgrey?logo=windows)

A tiny, zero-dependency Rust binary that triggers [WezTerm](https://wezfurlong.org/wezterm/)'s visual bell from [Claude Code](https://docs.anthropic.com/en/docs/claude-code) hooks, with project-aware toast notifications and per-pane targeting.

## Overview

| Property | Value |
|----------|-------|
| Language | Rust (edition 2021) |
| Dependencies | Zero |
| Build | `cargo build --release` with `strip` + LTO |
| Platform | Windows (ConPTY / WezTerm) |
| Hook events | `Stop`, `Notification` |
| Subagent filtering | Yes — Task tool agents are ignored |

## Problem

Claude Code captures all stdout/stderr from hook commands, so `printf '\a'` (BEL character) never reaches WezTerm. Writing directly to `CONOUT$` via Win32 API also fails because ConPTY doesn't relay control characters from the console screen buffer to WezTerm's VT parser.

With multiple projects open in different panes, you also need to know **which project** finished and have the bell flash in the **correct pane**.

## How It Works

```
Claude Code hook (Stop/Notification)
  → bell.exe reads CLAUDE_PROJECT_DIR, WEZTERM_PANE, stdin JSON
  → writes ~/.claude/bell_signal (project name, pane ID, event, message)
  → WezTerm Lua timer polls for the file every 1s
  → finds pane by ID → pane:inject_output('\x07')
  → shows toast notification with project name + event info
```

Subagent events (Task tool agents) are automatically filtered out — only main conversation stops and notifications trigger the bell.

## Features

| Feature | Description |
|---------|-------------|
| Visual bell | Injects `\x07` BEL into the correct WezTerm pane |
| Toast notifications | Shows project name + event message via WezTerm's notification API |
| Per-pane targeting | Uses `WEZTERM_PANE` to flash the exact pane that launched Claude |
| Subagent filtering | Skips `hook_event_name` values from Task tool sub-agents |
| Fallback | If pane ID is missing or not found, falls back to the active pane |

## Requirements

This project works with stock WezTerm for the bell and toast notifications. However, per-pane **header bars** (showing the project name in each pane) require a [custom WezTerm fork](https://github.com/SShadowS/wezterm) that adds `pane:set_header()` / `pane:get_header()` Lua API and a `format-pane-header` callback. See commit [`ac4d795`](https://github.com/SShadowS/wezterm/commit/ac4d79511f72f0560b7b5ae1964962e75dcf4c48) for details.

## Setup

### 1. Build & Deploy

```bash
git clone https://github.com/SShadowS/Wezterm.Bell ~/.claude/bell
cd ~/.claude/bell
cargo build --release
```

Or if you have the repo elsewhere, use the included build script which builds and copies the binary to `~/.claude/bell/`:

```bash
bash build.sh
```

### 2. Configure Claude Code Hooks

Add to `~/.claude/settings.json`:

```json
{
  "hooks": {
    "Notification": [
      {
        "hooks": [
          { "type": "command", "command": "~/.claude/bell/target/release/bell.exe" }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          { "type": "command", "command": "~/.claude/bell/target/release/bell.exe" }
        ]
      }
    ]
  }
}
```

### 3. Add WezTerm Lua Polling

Add this snippet to your `~/.wezterm.lua`:

```lua
local bell_signal_path = wezterm.home_dir .. '/.claude/bell_signal'

local function poll_bell_signal()
  local f = io.open(bell_signal_path, 'r')
  if f then
    local content = f:read('*a')
    f:close()
    os.remove(bell_signal_path)

    local lines = {}
    for line in content:gmatch('[^\n]*') do
      table.insert(lines, line)
    end
    local project_name = (lines[1] and lines[1] ~= '') and lines[1] or 'Claude Code'
    local pane_id = lines[2] or ''
    local hook_event = lines[3] or ''
    local message = lines[4] or ''

    local toast_title = 'Claude Code - ' .. project_name
    local toast_message
    if hook_event == 'Stop' then
      toast_message = 'Finished responding'
    elseif hook_event == 'Notification' and message ~= '' then
      toast_message = message
    else
      toast_message = 'Needs attention'
    end

    -- Find the target pane by ID and inject bell
    local found = false
    if pane_id ~= '' then
      for _, w in ipairs(wezterm.gui.gui_windows()) do
        for _, mux_tab in ipairs(w:mux_window():tabs()) do
          for _, pane in ipairs(mux_tab:panes()) do
            if tostring(pane:pane_id()) == pane_id then
              pane:inject_output('\x07')
              w:toast_notification(toast_title, toast_message, nil, 4000)
              found = true
            end
          end
        end
      end
    end

    -- Fallback: inject into active pane of each window
    if not found then
      for _, w in ipairs(wezterm.gui.gui_windows()) do
        local pane = w:active_pane()
        if pane then
          pane:inject_output('\x07')
          w:toast_notification(toast_title, toast_message, nil, 4000)
        end
      end
    end
  end
  wezterm.time.call_after(1, poll_bell_signal)
end
wezterm.time.call_after(1, poll_bell_signal)
```

## Signal File Format

`bell.exe` writes `~/.claude/bell_signal` as a plain-text file. WezTerm reads and deletes it on the next poll cycle.

| Line | Source | Fallback |
|------|--------|----------|
| 1 — project name | `CLAUDE_PROJECT_DIR` (basename) | `"Claude Code"` |
| 2 — pane ID | `WEZTERM_PANE` env var | empty string |
| 3 — hook event | `hook_event_name` from stdin JSON | empty string |
| 4 — message | `message` from stdin JSON | empty string |

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Reads env/stdin, writes signal file |
| `build.sh` | Builds release binary and copies to `~/.claude/bell/` |
| `Cargo.toml` | Crate config — zero dependencies, strip + LTO enabled |

## Why Not CONOUT$?

Two Win32 approaches that both failed:

| Approach | Why it fails |
|----------|-------------|
| `WriteConsoleW` to `CONOUT$` | Writes to the console character buffer; ConPTY does not relay BEL |
| `WriteFile` to `CONOUT$` | Writes raw bytes to the screen buffer; ConPTY diffs the buffer but drops control characters |

The file-signal + `inject_output` approach is the only reliable path on Windows with ConPTY.

---

**Author:** Torben Leth — [sshadows@sshadows.dk](mailto:sshadows@sshadows.dk)  
**License:** MIT
