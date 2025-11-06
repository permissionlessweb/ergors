#!/bin/bash

# Recursive Agentic Network Dependencies Installation Script
# Installs Docker, Rust, Python 3, Go, Git, Ollama, VLLM, 
# and additional CLI tools
# Supports Ubuntu/Debian, Fedora/CentOS, Arch Linux, and macOS
# PYTHON REQUIREMENTS: 
# RUST REQUIREMENTS: 
# GO REQUIREMENTS: 
# DOCKER REQUIREMENTS
# GIT REQUIREMENTS

set -euo pipefail

# Color codes for output
readonly RED='\e[31m'
readonly GREEN='\e[32m'
readonly YELLOW='\e[33m'
readonly BLUE='\e[34m'
readonly RESET='\e[0m'

# Minimum required versions
readonly MIN_PYTHON_VERSION="3.9"
readonly MIN_GO_VERSION="1.19"

# Print colored messages
print_success() { printf "${GREEN}✓ %s${RESET}\n" "$1"; }
print_info() { printf "${BLUE}ℹ %s${RESET}\n" "$1"; }
print_warning() { printf "${YELLOW}⚠ %s${RESET}\n" "$1"; }
print_error() { printf "${RED}✗ %s${RESET}\n" "$1" >&2; }

# Check if sudo is available
check_sudo() {
    if ! command -v sudo >/dev/null 2>&1; then
        print_error "sudo is required but not available. Please install sudo or run as root."
        exit 1
    fi

    if ! sudo -n true 2>/dev/null; then
        print_info "This script requires sudo privileges. You may be prompted for your password."
        if ! sudo true; then
            print_error "Failed to obtain sudo privileges. Exiting."
            exit 1
        fi
    fi
    print_success "Sudo access confirmed"
}

# Detect operating system and package manager
detect_os() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        if command -v brew >/dev/null 2>&1; then
            echo "macos"
        else
            print_error "macOS detected but Homebrew is not installed. Please install Homebrew first."
            exit 1
        fi
    elif [[ -f /etc/os-release ]]; then
        . /etc/os-release
        case "$ID" in
            ubuntu|debian) echo "debian" ;;
            fedora|centos|rhel) echo "fedora" ;;
            arch|manjaro) echo "arch" ;;
            *)
                print_error "Unsupported Linux distribution: $ID"
                exit 1
                ;;
        esac
    else
        print_error "Unable to detect operating system"
        exit 1
    fi
}

# Version comparison function
version_ge() {
    printf '%s\n%s\n' "$2" "$1" | sort -V -C
}

# Check if Python version meets minimum requirement
check_python_version() {
    local python_cmd="$1"
    local version
    version=$($python_cmd --version 2>&1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || echo "0.0.0")
    version_ge "$version" "$MIN_PYTHON_VERSION"
}

# Install packages based on OS
install_package() {
    local package="$1"
    local os="$2"

    case "$os" in
        debian)
            sudo apt-get update -qq
            sudo apt-get install -y "$package"
            ;;
        fedora)
            sudo dnf install -y "$package"
            ;;
        arch)
            sudo pacman -Sy --noconfirm "$package"
            ;;
        macos)
            brew install "$package"
            ;;
    esac
}

# Install Docker
install_docker() {
    local os="$1"

    if command -v docker >/dev/null 2>&1; then
        print_success "Docker is already installed"
        return 0
    fi

    print_info "Installing Docker..."

    case "$os" in
        debian)
            # Install Docker using official repository
            sudo apt-get update -qq
            sudo apt-get install -y ca-certificates curl gnupg lsb-release
            sudo mkdir -p /etc/apt/keyrings
            curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg
            echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu $(lsb_release -cs) stable" | sudo tee /etc/apt/sources.list.d/docker.list >/dev/null
            sudo apt-get update -qq
            sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin
            ;;
        fedora)
            sudo dnf install -y dnf-plugins-core
            sudo dnf config-manager --add-repo https://download.docker.com/linux/fedora/docker-ce.repo
            sudo dnf install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin
            ;;
        arch)
            sudo pacman -Sy --noconfirm docker docker-compose
            ;;
        macos)
            print_warning "On macOS, please install Docker Desktop manually from https://www.docker.com/products/docker-desktop"
            return 0
            ;;
    esac

    # Enable and start Docker service (not needed on macOS)
    if [[ "$os" != "macos" ]]; then
        sudo systemctl enable docker
        sudo systemctl start docker
        # Add current user to docker group
        sudo usermod -aG docker "$USER"
        print_warning "You may need to log out and back in for Docker group membership to take effect"
    fi

    print_success "Docker installed successfully"
}

# Install Rust via rustup
install_rust() {
    if command -v rustc >/dev/null 2>&1 && command -v cargo >/dev/null 2>&1; then
        print_success "Rust and Cargo are already installed"
        return 0
    fi

    print_info "Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

    # Source the cargo environment
    source "$HOME/.cargo/env" 2>/dev/null || export PATH="$HOME/.cargo/bin:$PATH"

    print_success "Rust and Cargo installed successfully"
}

# Install Python 3
install_python() {
  local os="$1"
  # Check if Python 3 with adequate version is available
  for python_cmd in python3 python; do
   case "$os" in
    debian)
      sudo apt-get update -qq
      if ! sudo apt-get install -y python3 python3-pip python3-venv pipx; then
        print_error "Failed to install Python 3 on Debian"
        exit 1
      fi
      ;;
    fedora)
      if ! sudo dnf install -y python3 python3-pip; then
        print_error "Failed to install Python 3 on Fedora"
        exit 1
      fi
      ;;
    arch)
      if ! sudo pacman -Sy --noconfirm python python-pip; then
        print_error "Failed to install Python 3 on Arch"
        exit 1
      fi
      ;;
    macos)
      if ! brew install python3; then
        print_error "Failed to install Python 3 on macOS"
        exit 1
      fi
      ;;
  esac
    if command -v "$python_cmd" >/dev/null 2>&1; then
      if check_python_version "$python_cmd"; then
        print_success "Python $($python_cmd --version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+') is already installed"
        return 0
      fi
    fi
  done
  print_info "Installing Python 3..."
 
  print_success "Python 3 installed successfully"
}

# Install Go
install_go() {
  if command -v go >/dev/null 2>&1; then
    local go_version
    go_version=$(go version | grep -oE 'go[0-9]+\.[0-9]+\.[0-9]+' | sed 's/go//')
    if version_ge "$go_version" "$MIN_GO_VERSION"; then
      print_success "Go $go_version is already installed"
      return 0
    fi
  fi
  print_info "Installing Go..."
  case "$(detect_os)" in
    macos)
      brew install go
      ;;
    *)
      # Download and install Go for Linux
      local go_version="1.20.5"  # Update this to the latest version
      local go_tarball="go${go_version}.linux-amd64.tar.gz"
      local go_url="https://go.dev/dl/${go_tarball}"
      # Remove existing Go installation
      sudo rm -rf /usr/local/go
      # Download and extract Go
      curl -fsSL "$go_url" | sudo tar -C /usr/local -xzf -
      if [ $? -ne 0 ]; then
        print_error "Failed to download and extract Go"
        exit 1
      fi
      # Add Go to PATH if not already there
      if ! grep -q "/usr/local/go/bin" ~/.bashrc 2>/dev/null; then
        echo 'export PATH=$PATH:/usr/local/go/bin' >> ~/.bashrc
      fi
      export PATH=$PATH:/usr/local/go/bin
      ;;
  esac
  print_success "Go installed successfully"
}

# Install Git
install_git() {
    local os="$1"

    if command -v git >/dev/null 2>&1; then
        print_success "Git is already installed"
        return 0
    fi

    print_info "Installing Git..."
    install_package "git" "$os"
    print_success "Git installed successfully"
}

# Install and configure SSH server
install_ssh_server() {
    local os="$1"

    case "$os" in
        debian)
            if systemctl is-active --quiet ssh || systemctl is-active --quiet sshd; then
                print_success "SSH server is already running"
                return 0
            fi

            print_info "Installing OpenSSH server..."
            sudo apt-get update -qq
            sudo apt-get install -y openssh-server
            sudo systemctl enable ssh
            sudo systemctl start ssh
            ;;
        fedora)
            if systemctl is-active --quiet sshd; then
                print_success "SSH server is already running"
                return 0
            fi

            print_info "Installing OpenSSH server..."
            sudo dnf install -y openssh-server
            sudo systemctl enable sshd
            sudo systemctl start sshd
            ;;
        arch)
            if systemctl is-active --quiet sshd; then
                print_success "SSH server is already running"
                return 0
            fi

            print_info "Installing OpenSSH server..."
            sudo pacman -Sy --noconfirm openssh
            sudo systemctl enable sshd
            sudo systemctl start sshd
            ;;
        macos)
            print_info "Enabling SSH server on macOS..."
            sudo systemsetup -setremotelogin on
            ;;
    esac

    print_success "SSH server installed and started"
}

# Install Ollama
install_ollama() {
    if command -v ollama >/dev/null 2>&1; then
        print_success "Ollama is already installed"
        return 0
    fi

    print_info "Installing Ollama..."
    curl -fsSL https://ollama.com/install.sh | sh
    print_success "Ollama installed successfully"
}

# Pull Ollama model
pull_ollama_model() {
    print_info "Pulling Ollama model (llama2)..."

    # Start Ollama service if not running
    if ! pgrep -f ollama >/dev/null; then
        print_info "Starting Ollama service..."
        ollama serve &
        sleep 5
    fi

    # Pull the model
    ollama pull llama2
    print_success "Ollama model (llama2) pulled successfully"
}

# Install goose CLI
install_goose() {
    if command -v goose >/dev/null 2>&1; then
        print_success "Goose CLI is already installed"
        return 0
    fi

    print_info "Installing Goose CLI..."

    # Ensure Go is in PATH
    export PATH=$PATH:/usr/local/go/bin:$HOME/.cargo/bin

    if command -v go >/dev/null 2>&1; then
        go install github.com/pressly/goose/v3/cmd/goose@latest
    else
        print_warning "Go not found in PATH, trying to install goose via package manager..."
        case "$(detect_os)" in
            macos)
                brew install goose
                ;;
            *)
                print_warning "Please ensure Go is installed and in PATH to install goose"
                return 1
                ;;
        esac
    fi

    print_success "Goose CLI installed successfully"
}

# Install claude-code CLI
install_claude_code() {
    if command -v claude-code >/dev/null 2>&1; then
        print_success "Claude Code CLI is already installed"
        return 0
    fi

    print_info "Installing Claude Code CLI..."

    case "$(detect_os)" in
        macos)
            brew install anthropics/claude/claude-code
            ;;
        *)
            # Install via npm if available, otherwise provide instructions
            if command -v npm >/dev/null 2>&1; then
                npm install -g claude-code
            else
                print_info "Installing Node.js and npm first..."
                case "$(detect_os)" in
                    debian)
                        curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash -
                        sudo apt-get install -y nodejs
                        ;;
                    fedora)
                        sudo dnf install -y nodejs npm
                        ;;
                    arch)
                        sudo pacman -Sy --noconfirm nodejs npm
                        ;;
                esac
                npm install -g claude-code
            fi
            ;;
    esac

    print_success "Claude Code CLI installed successfully"
}

# Install qwen-code CLI
install_qwen_code() {
    if command -v qwen-code >/dev/null 2>&1; then
        print_success "Qwen Code CLI is already installed"
        return 0
    fi

    print_info "Installing Qwen Code CLI..."

    # Try to install via pip
    if command -v pip3 >/dev/null 2>&1; then
        pip3 install --user qwen-code || pip3 install qwen-code
    elif command -v pip >/dev/null 2>&1; then
        pip install --user qwen-code || pip install qwen-code
    else
        print_warning "pip not found. Please install Python and pip first."
        return 1
    fi

    print_success "Qwen Code CLI installed successfully"
}

# Main installation function
main() {
    print_info "Starting Recursive Agentic Network dependencies installation..."

    # Check sudo availability
    check_sudo

    # Detect operating system
    local os
    os=$(detect_os)
    print_info "Detected OS: $os"

    # Install all dependencies
    install_git "$os"
    install_python "$os"
    install_go "$os"
    install_rust
    install_docker "$os"
    install_ssh_server "$os"
    install_ollama
    install_goose
    install_claude_code
    install_qwen_code

    # Pull Ollama model
    pull_ollama_model

    print_success "All dependencies installed successfully!"
    print_info "You may need to restart your terminal or log out/in for all changes to take effect."

    # Display installed versions
    echo ""
    print_info "Installed versions:"
    command -v git >/dev/null && echo "Git: $(git --version)"
    command -v python3 >/dev/null && echo "Python: $(python3 --version)"
    command -v go >/dev/null && echo "Go: $(go version)"
    command -v rustc >/dev/null && echo "Rust: $(rustc --version)"
    command -v docker >/dev/null && echo "Docker: $(docker --version)"
    command -v ollama >/dev/null && echo "Ollama: $(ollama --version 2>/dev/null || echo 'installed')"
    command -v goose >/dev/null && echo "Goose: installed"
    command -v claude-code >/dev/null && echo "Claude Code: installed"
    command -v qwen-code >/dev/null && echo "Qwen Code: installed"
}

# Run main function
main "$@"