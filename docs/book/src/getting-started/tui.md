# TUI

The ZeroClaw TUI is a terminal interface for managing configuration, chatting with agents, and monitoring your daemon. It connects over a Unix socket for local use, or over WebSocket Secure (WSS) for remote use.

## Local setup

On the same machine as the daemon, no extra configuration is needed:

```bash
zeroclaw-tui
```

The TUI finds the daemon socket automatically at `<data_dir>/data/daemon.sock`. If the daemon isn't running, the TUI spawns an ephemeral one.

## Remote setup (WSS)

Connect a TUI on your workstation to a daemon running on another machine (Raspberry Pi, home server, VPS, etc.).

### On the remote host (daemon side)

1. **Generate a self-signed TLS certificate:**

   ```bash
   openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 \
     -keyout ~/.zeroclaw/wss.key \
     -out ~/.zeroclaw/wss.cert \
     -days 3650 -nodes -subj '/CN=zeroclaw'
   ```

2. **Enable WSS in `~/.zeroclaw/config.toml`:**

   ```toml
   [wss]
   enabled = true
   cert_path = "/home/youruser/.zeroclaw/wss.cert"
   key_path = "/home/youruser/.zeroclaw/wss.key"
   ```

   Use absolute paths. The config does not expand `~`.

3. **Open the firewall port:**

   ```bash
   sudo ufw allow 9781/tcp
   ```

   The default WSS port is **9781**. Change it with `port = <number>` in the `[wss]` section.

4. **Start (or restart) the daemon:**

   ```bash
   zeroclaw daemon
   ```

   You should see a log line confirming the WSS listener started on `0.0.0.0:9781`.

### On your workstation (TUI side)

5. **Connect with TLS verification skipped:**

   ```bash
   zeroclaw-tui --connect wss://<remote-ip>:9781 --tls-skip-verify
   ```

   `--tls-skip-verify` is required for self-signed certificates. The HMAC session signing still authenticates the connection.

That's it. The TUI reconnects automatically if the connection drops.

## Config reference

The `[wss]` section in `config.toml`:

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `false` | Enable the WSS listener |
| `bind` | `0.0.0.0` | Bind address |
| `port` | `9781` | Listen port |
| `cert_path` | (none) | Absolute path to PEM certificate |
| `key_path` | (none) | Absolute path to PEM private key |

## CLI flags

| Flag | Description |
|------|-------------|
| `--connect <url>` | Connect to a remote daemon via WSS (e.g. `wss://host:9781`) |
| `--tls-skip-verify` | Skip TLS certificate verification. Required for self-signed certs |
| `--config-dir <path>` | Override the config directory |
| `-a, --agent <alias>` | Start in chat mode with this agent |
