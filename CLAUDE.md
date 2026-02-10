# Wezterm.Bell

A tiny Rust binary that triggers WezTerm's visual bell from Claude Code hooks, with project-aware toast notifications and per-pane targeting.

## Problem

Claude Code captures all stdout/stderr from hook commands, so `printf '\a'` (BEL character) never reaches WezTerm. Writing directly to `CONOUT$` via Win32 API also fails because ConPTY doesn't relay control characters from the console screen buffer to WezTerm's VT parser.

Additionally, with multiple projects open, the user needs to know **which project** finished, and the bell should flash in the **correct pane**, not whichever one happens to be focused.

## Solution

File-based signaling with WezTerm Lua polling:

1. **`bell.exe`** reads context (project name, pane ID, hook event, message) and writes a signal file at `~/.claude/bell_signal`
2. **WezTerm Lua timer** polls every 1s, detects the file, parses it, deletes it, and:
   - Finds the specific pane by ID and calls `pane:inject_output('\x07')` to feed BEL into the VT parser
   - Shows a **toast notification** with the project name and event details

## Architecture

```
Claude Code hook (Stop/Notification)
  → bell.exe reads CLAUDE_PROJECT_DIR, WEZTERM_PANE, stdin JSON
  → bell.exe writes ~/.claude/bell_signal (project, pane ID, event, message)
  → WezTerm Lua timer detects file, parses it, deletes it
  → Finds pane by ID → pane:inject_output('\x07') on that pane only
  → window:toast_notification() with project name + event info
```

## Signal File Format

```
line 1: project name (from CLAUDE_PROJECT_DIR, fallback: "Claude Code")
line 2: pane ID (from WEZTERM_PANE, fallback: empty)
line 3: hook event name (from stdin JSON hook_event_name)
line 4: message (from stdin JSON message)
```

## Files

### This repo
- `Cargo.toml` — zero-dependency Rust project (raw `std::fs` only)
- `src/main.rs` — reads context from env vars + stdin JSON, writes signal file to `%USERPROFILE%/.claude/bell_signal`

### Deployment (outside this repo)
- **`~/.claude/bell/`** — working copy where `cargo build --release` is run
- **`~/.claude/settings.json`** — Claude Code hooks configuration:
  ```json
  {
    "hooks": {
      "Notification": [{ "hooks": [{ "type": "command", "command": "~/.claude/bell/target/release/bell.exe" }] }],
      "Stop": [{ "hooks": [{ "type": "command", "command": "~/.claude/bell/target/release/bell.exe" }] }]
    }
  }
  ```
- **`~/.wezterm.lua`** — polling timer + pane targeting + toast notification snippet:
  ```lua
  local bell_signal_path = 'C:/Users/SShadowS/.claude/bell_signal'
  local function poll_bell_signal()
    local f = io.open(bell_signal_path, 'r')
    if f then
      local content = f:read('*a')
      f:close()
      os.remove(bell_signal_path)

      -- Parse signal file lines
      local lines = {}
      for line in content:gmatch('[^\n]*') do
        table.insert(lines, line)
      end
      local project_name = (lines[1] and lines[1] ~= '') and lines[1] or 'Claude Code'
      local pane_id = lines[2] or ''
      local hook_event = lines[3] or ''
      local message = lines[4] or ''

      -- Build toast content
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
            for _, pane in ipairs(tab:panes()) do
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

## Build & Install

```bash
cd ~/.claude/bell
cargo build --release
# Binary at: ~/.claude/bell/target/release/bell.exe
```

## Why not CONOUT$?

We tried two Win32 approaches that both failed:
- **`WriteConsoleW` to `CONOUT$`** — writes to console character buffer, ConPTY doesn't relay BEL
- **`WriteFile` to `CONOUT$`** — writes raw bytes to screen buffer, ConPTY diffs the buffer but drops control characters

The file-signal + `inject_output` approach is the only reliable path on Windows with ConPTY.
