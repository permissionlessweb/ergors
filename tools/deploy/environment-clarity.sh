#!/bin/sh
#===============================================================
# unbintu-env‑collector.sh
#
# Collects information about the host that runs an "unbintu"
# agentic workflow. The output is a single JSON object printed
# to STDOUT, suitable for piping into jq or feeding directly to
# an LLM.
#
# Features gathered:
#   • OS / kernel details
#   • CPU / core count
#   • Memory (total / free / swap)
#   • GPU(s) – NVIDIA (nvidia‑smi) and AMD (rocm‑smi)
#   • Disk usage (root + any mounted /data directories)
#   • Network interfaces & IP addresses (IPv4 & IPv6)
#   • Docker / Podman status (if containers are used)
#   • Kubernetes context (if kubectl is present)
#   • Environment variables that start with UNBITU_ (or all if you prefer)
#
# The script is deliberately defensive – if a command is missing
# it records a `null` value rather than aborting.
#===============================================================

# ---------- Helper functions ----------
# Echo a JSON string value safely (escapes embedded quotes & backslashes)
json_escape() {
    # $1 = raw string
    printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' -e "s/'/\\'/g"
}

# Execute a command if it exists, otherwise return empty string
run_if_exists() {
    if command -v "$1" >/dev/null 2>&1; then
        shift
        "$@"
    else
        echo ""
    fi
}

# ---------- 1. OS / Kernel ----------
KERNEL=$(uname -r 2>/dev/null || echo "")
OS_NAME=$(run_if_exists lsb_release -ds)
# Fallback to /etc/os-release when lsb_release is not present
if [ -z "$OS_NAME" ] && [ -f /etc/os-release ]; then
    OS_NAME=$( . /etc/os-release && printf '%s' "$PRETTY_NAME" )
fi

# ---------- 2. CPU ----------
CPU_MODEL=$(run_if_exists grep -m1 -i 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ *//')
CPU_CORES=$(run_if_exists nproc || echo "1")

# ---------- 3. Memory ----------
MEM_TOTAL=$(run_if_exists awk '/MemTotal/ {printf "%.0f",$2/1024}' /proc/meminfo)   # MB
MEM_FREE=$(run_if_exists awk '/MemAvailable/ {printf "%.0f",$2/1024}' /proc/meminfo) # MB
SWAP_TOTAL=$(run_if_exists awk '/SwapTotal/ {printf "%.0f",$2/1024}' /proc/meminfo)  # MB
SWAP_FREE=$(run_if_exists awk '/SwapFree/ {printf "%.0f",$2/1024}' /proc/meminfo)   # MB

# ---------- 4. GPUs ----------
# NVIDIA -------------------------------------------------
if command -v nvidia-smi >/dev/null 2>&1; then
    NVIDIA_COUNT=$(nvidia-smi --query-gpu=name --format=csv,noheader | wc -l | tr -d ' ')
    NVIDIA_DEVS=$(nvidia-smi --query-gpu=name,driver_version,memory.total --format=csv,noheader \
                | awk -F, '{printf "{\"name\":\"%s\",\"driver\":\"%s\",\"memory_mb\":%d}", $1, $2, $3*1024}')
else
    NVIDIA_COUNT=0
    NVIDIA_DEVS=""
fi

# AMD (ROCm) ---------------------------------------------
if command -v rocm-smi >/dev/null 2>&1; then
    AMD_COUNT=$(rocm-smi -i | grep -c '^GPU')
    AMD_DEVS=$(rocm-smi -i | awk '
        /^GPU/ {gpu=$1}
        /GPU Memory/ {mem=$4}
        /GPU Vendor/ {vendor=$3}
        END {printf "{\"name\":\"%s\",\"vendor\":\"%s\",\"memory_mb\":%d}", gpu, vendor, mem}')
else
    AMD_COUNT=0
    AMD_DEVS=""
fi

# ---------- 5. Disk -------------------------------------------------
# Helper to produce an array of JSON objects for each mount point
disk_json() {
    df -P -B1 "$@" 2>/dev/null | tail -n +2 | awk '
    {
        total=$2; used=$3; avail=$4; mp=$6;
        printf "{\"mount\":\"%s\",\"total_bytes\":%s,\"used_bytes\":%s,\"available_bytes\":%s},", mp, total, used, avail
    }' | sed 's/,$//'
}

DISK_ROOT=$(disk_json /)               # root filesystem
DISK_DATA=$(disk_json /data 2>/dev/null)   # optional /data mount

# ---------- 6. Network -------------------------------------------------
# Collect all non‑loopback interfaces
NET_IFACES=$(ip -o -4 addr show up 2>/dev/null | awk '{print $2}' | sort -u)
NET_JSON=""

for IF in $NET_IFACES; do
    IPV4=$(ip -o -4 addr show dev "$IF" | awk '{print $4}' | cut -d/ -f1)
    IPV6=$(ip -o -6 addr show dev "$IF" | awk '{print $4}' | cut -d/ -f1' | head -n1)
    # Build per‑interface JSON
    NET_JSON="${NET_JSON}{\"iface\":\"$IF\",\"ipv4\":\"$IPV4\",\"ipv6\":\"$IPV6\"},"
done
# Strip trailing comma
NET_JSON=$(printf '%s' "$NET_JSON" | sed 's/,$//')

# ---------- 7. Docker / Podman -----------------------------------------
if command -v docker >/dev/null 2>&1; then
    DOCKER_RUNNING=$(docker info --format '{{.ServerVersion}}' 2>/dev/null || echo "null")
    DOCKER_CONTAINERS=$(docker ps -q | wc -l | tr -d ' ')
else
    DOCKER_RUNNING=null
    DOCKER_CONTAINERS=0
fi

if command -v podman >/dev/null 2>&1; then
    PODMAN_RUNNING=$(podman info --format '{{.Host.Version}}' 2>/dev/null || echo "null")
    PODMAN_CONTAINERS=$(podman ps -q | wc -l | tr -d ' ')
else
    PODMAN_RUNNING=null
    PODMAN_CONTAINERS=0
fi

# ---------- 8. Kubernetes ---------------------------------------------
if command -v kubectl >/dev/null 2>&1; then
    KUBE_CONTEXT=$(kubectl config current-context 2>/dev/null || echo "null")
    KUBE_CLUSTER=$(kubectl config view -o jsonpath='{.contexts[?(@.name=="'"$KUBE_CONTEXT"'")].context.cluster}' 2>/dev/null || echo "null")
else
    KUBE_CONTEXT=null
    KUBE_CLUSTER=null
fi

# ---------- 9. UNBITU_* environment variables -------------------------
UNBITU_ENV=$(env | grep '^UNBITU_' | awk -F= '
{
    gsub(/\\/,"\\\\",$2); gsub(/"/,"\\\"",$2);
    printf "\"%s\":\"%s\",", $1, $2
}' | sed 's/,$//')
# If you want *all* env vars, replace the pipe above with just `env | awk -F= …`

# ---------- Assemble final JSON ---------------------------------------
printf '{\n'
printf '  "kernel":"%s",\n' "$(json_escape "$KERNEL")"
printf '  "os_name":"%s",\n' "$(json_escape "$OS_NAME")"
printf '  "cpu":{\n'
printf '    "model":"%s",\n' "$(json_escape "$CPU_MODEL")"
printf '    "cores":%s\n' "$CPU_CORES"
printf '  },\n'
printf '  "memory":{\n'
printf '    "total_mb":%s,\n' "$MEM_TOTAL"
printf '    "available_mb":%s,\n' "$MEM_FREE"
printf '    "swap_total_mb":%s,\n' "$SWAP_TOTAL"
printf '    "swap_free_mb":%s\n' "$SWAP_FREE"
printf '  },\n'
printf '  "gpus":{\n'
printf '    "nvidia":{ "count":%s, "details":[%s] },\n' "$NVIDIA_COUNT" "$NVIDIA_DEVS"
printf '    "amd":{ "count":%s, "details":[%s] }\n' "$AMD_COUNT" "$AMD_DEVS"
printf '  },\n'
printf '  "disks":{\n'
printf '    "root":[%s],\n' "$(printf '%s' "$DISK_ROOT")"
if [ -n "$DISK_DATA" ]; then
    printf '    "data":[%s]\n' "$(printf '%s' "$DISK_DATA")"
else
    printf '    "data":[]\n'
fi
printf '  },\n'
printf '  "network":{\n'
printf '    "interfaces":[%s]\n' "$NET_JSON"
printf '  },\n'
printf '  "container_runtime":{\n'
printf '    "docker":{ "server_version":"%s", "running_containers":%s },\n' "$(json_escape "$DOCKER_RUNNING")" "$DOCKER_CONTAINERS"
printf '    "podman":{ "server_version":"%s", "running_containers":%s }\n' "$(json_escape "$PODMAN_RUNNING")" "$PODMAN_CONTAINERS"
printf '  },\n'
printf '  "kubernetes":{\n'
printf '    "current_context":"%s",\n' "$(json_escape "$KUBE_CONTEXT")"
printf '    "cluster":"%s"\n' "$(json_escape "$KUBE_CLUSTER")"
printf '  },\n'
printf '  "unbintu_env":{ %s }\n' "$UNBITU_ENV"
printf '}\n'