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
   - Marks the tab in `bell_tabs` for **tab bell indicator** (bell icon on inactive tabs, auto-expires after 30s)

## Architecture

```
Claude Code hook (Stop/Notification)
  → bell.exe reads CLAUDE_PROJECT_DIR, WEZTERM_PANE, stdin JSON
  → bell.exe writes ~/.claude/bell_signal (project, pane ID, event, message)
  → WezTerm Lua timer detects file, parses it, deletes it
  → Finds pane by ID → pane:inject_output('\x07') on that pane only
  → Marks tab in bell_tabs if not active → tabline shows bell icon
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
- **`~/.wezterm.lua`** — bell state tracking + tabline bell indicator + polling timer + pane targeting + toast notification:
  ```lua
  -- Track tabs with pending Claude Code bell notifications
  -- Key: tab_id (number), Value: os.time() timestamp
  local bell_tabs = {}

  -- In tabline.setup(), tab_active and tab_inactive sections use function components:
  -- tab_active: clears bell_tabs[tab.tab_id] on focus (returns '')
  -- tab_inactive: shows md_bell_badge_outline icon if bell_tabs entry exists and <= 30s old

  local bell_signal_path = 'C:/Users/SShadowS/.claude/bell_signal'
  local function poll_bell_signal()
    local f = io.open(bell_signal_path, 'r')
    if f then
      local content = f:read('*a')
      f:close()
      os.remove(bell_signal_path)

      -- Parse signal file lines: project_name, pane_id, hook_event, message
      local lines = {}
      for line in content:gmatch('[^\n]*') do
        table.insert(lines, line)
      end
      local project_name = (lines[1] and lines[1] ~= '') and lines[1] or 'Claude Code'
      local pane_id = lines[2] or ''
      local hook_event = lines[3] or ''
      local message = lines[4] or ''

      -- Find the target pane by ID
      local target_pane = nil
      local target_window = nil
      if pane_id ~= '' then
        for _, w in ipairs(wezterm.gui.gui_windows()) do
          for _, mux_tab in ipairs(w:mux_window():tabs()) do
            for _, pane in ipairs(mux_tab:panes()) do
              if tostring(pane:pane_id()) == pane_id then
                target_pane = pane
                target_window = w
              end
            end
          end
        end
      end

      -- Fallback to active pane
      if not target_pane then
        for _, w in ipairs(wezterm.gui.gui_windows()) do
          local pane = w:active_pane()
          if pane then
            target_pane = pane
            target_window = w
            break
          end
        end
      end

      if target_pane then
        if hook_event == 'Stop' or hook_event == 'Notification' then
          target_pane:inject_output('\x07')

          -- Track bell for tab highlighting
          local tab = target_pane:tab()
          if tab then
            local active_tab = target_window:active_tab()
            if active_tab and active_tab:tab_id() ~= tab:tab_id() then
              bell_tabs[tab:tab_id()] = os.time()
            end
          end

          local toast_title = 'Claude Code - ' .. project_name
          local toast_message
          if hook_event == 'Stop' then
            toast_message = 'Finished responding'
          elseif message ~= '' then
            toast_message = message
          else
            toast_message = 'Needs attention'
          end
          target_window:toast_notification(toast_title, toast_message, nil, 4000)
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
