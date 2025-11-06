# CW-AGENT Config Manager

A Textual-based TUI for creating and managing CW-AGENT configuration files with validation and guided workflows.

## Quick Start

```bash
# Install dependencies
cd ui/config_manager
pip install -r requirements.txt

# Launch config manager
python main.py

# Or specify config directory
python main.py --config-dir /path/to/configs
```

## Usage

### Navigation
- **Arrow keys**: Navigate sections and fields
- **Tab**: Switch between panels  
- **Enter**: Edit field or enter subsection
- **Escape**: Go back or cancel
- **Ctrl+S**: Save configuration
- **Ctrl+Q**: Quit

### Supported Files
- `config.toml` - Node configuration (identity, network, storage, LLM)
- `api-keys.json` - LLM provider credentials
- `ssh-config.json` - Remote host connections

### Features
- **Real-time validation** with error highlighting
- **Array management** for LLM providers and peer lists
- **Template loading** for different node types
- **Automatic backup** before changes
- **Golden ratio compliance** checking

### Example Workflow
1. Launch: `python main.py`
2. Select configuration file from tree
3. Navigate to section (identity, network, etc.)
4. Edit fields with type-specific editors
5. Save with Ctrl+S after validation passes

## Command Options

```bash
python main.py [OPTIONS]

Options:
  -d, --config-dir PATH  Configuration directory [default: .]
  --debug               Run in debug mode  
  --help                Show help message
```

## Development

```bash
# Run with hot reload
python main.py --debug

# File structure:
# - main.py: Entry point
# - app.py: Main application 
# - models/: Configuration schemas
# - widgets/: UI components
# - validation/: Validation engine
```