# tmux for AI Agents

## Session Commands

| Operation | Command | Notes |
|-----------|---------|-------|
| Create session | `tmux new-session -d -s {name}` | `-d` = detached |
| List sessions | `tmux list-sessions` | Shows all sessions |
| Attach session | `tmux attach-session -t {name}` | Connect to session |
| Detach session | `tmux detach-client -s {name}` | Keeps running |
| Kill session | `tmux kill-session -t {name}` | Terminates |

## Command Execution

| Operation | Command | Notes |
|-----------|---------|-------|
| Send command | `tmux send-keys -t {name} "{cmd}" C-m` | `C-m` = Enter |
| Capture output | `tmux capture-pane -p -t {name}` | Visible content |
| Capture history | `tmux capture-pane -p -t {name} -S -{lines}` | With scrollback |

## Typical Workflow

```bash
# 1. Create detached session
tmux new-session -d -s mysession

# 2. Execute command
tmux send-keys -t mysession "ls -la" C-m

# 3. Wait for completion (sleep or poll)
sleep 1

# 4. Capture output
tmux capture-pane -p -t mysession

# 5. Clean up
tmux kill-session -t mysession
```

**Note:** Clear pane before commands for clean output: `tmux send-keys -t {name} "clear" C-m`
