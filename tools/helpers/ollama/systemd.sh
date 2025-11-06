#!/usr/bin/env bash
# ------------------------------------------------------------
# install_ollama_service.sh
# ------------------------------------------------------------
# PURPOSE : Install and enable a systemd service that runs the
#           Ollama binary (the API server) automatically.
# USAGE   : sudo ./install_ollama_service.sh
# REQUIRES: Bash, systemd, the `ollama` binary must already be
#           in the PATH (or you can point the script to it).
# ------------------------------------------------------------

set -euo pipefail   # Fail fast, treat unset vars as errors

# ---------- Helper functions ----------
log()   { printf '[%s] %s\n' "$(date +'%Y-%m-%d %H:%M:%S')" "$*"; }
die()   { log "ERROR: $*" >&2; exit 1; }

# ---------- Preconditions ----------
# Must be run as root (or via sudo)
if [[ "$(id -u)" -ne 0 ]]; then
    die "This script must be run as root.  Use sudo."
fi

# Find the ollama binary -------------------------------------------------
OLLAMA_BIN="$(command -v ollama || true)"
if [[ -z "$OLLAMA_BIN" ]]; then
    die "Could not locate the 'ollama' executable in $PATH."
fi

# Which user should run the service? ------------------------------------
# By default we run it as the user that originally invoked sudo (if any),
# otherwise fall back to root.
if [[ -n "${SUDO_USER:-}" ]]; then
    SERVICE_USER="${SUDO_USER}"
else
    SERVICE_USER="$(whoami)"   # will be root in this case
fi

# Verify that the chosen user actually exists
if ! id -u "$SERVICE_USER" >/dev/null 2>&1; then
    die "User \"$SERVICE_USER\" does not exist on this host."
fi

# ---------- Create the systemd unit file --------------------------------
UNIT_PATH="/etc/systemd/system/ollama.service"

log "Creating systemd unit at $UNIT_PATH (runs as $SERVICE_USER, binary $OLLAMA_BIN)"

cat >"$UNIT_PATH" <<EOF
[Unit]
Description=Ollama API Service
After=network.target

[Service]
Type=simple
User=${SERVICE_USER}
ExecStart=${OLLAMA_BIN} serve   # <-- adjust arguments if you need a different entry point
Restart=always
RestartSec=5
# Optional: write stdout/stderr to the journal (default) or a file
# StandardOutput=append:/var/log/ollama.log
# StandardError=append:/var/log/ollama.log

[Install]
WantedBy=multi-user.target
EOF

# ---------- Reload systemd, enable and start the service -----------------
log "Reloading systemd daemon"
systemctl daemon-reload

log "Enabling ollama.service to start at boot"
systemctl enable ollama.service

log "Starting ollama.service"
systemctl start ollama.service

# ---------- Verify the service is running --------------------------------
if systemctl is-active --quiet ollama.service; then
    log "✅ ollama.service is now active"
else
    log "⚠️ ollama.service failed to start – fetching status"
    systemctl status ollama.service --no-pager
    die "Service did not start correctly"
fi

# Optional: expose a quick health‑check (assumes default port 11434)
if command -v curl >/dev/null 2>&1; then
    sleep 2   # give the server a moment to bind
    if curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:11434 || true; then
        log "Ollama endpoint appears reachable (HTTP 200 expected)."
    else
        log "Could not reach Ollama HTTP endpoint – you may need to adjust firewall rules."
    fi
fi

log "All done.  Ollama will now run automatically on boot."
exit 0