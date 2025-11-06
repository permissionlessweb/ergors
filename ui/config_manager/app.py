"""Main Textual application for CW-AGENT configuration manager."""

from pathlib import Path
from typing import Optional, Dict, Any, List
import toml
import json
from datetime import datetime

from textual.app import App, ComposeResult
from textual.containers import Container, Horizontal, Vertical, ScrollableContainer
from textual.widgets import Header, Footer, Tree, Label, Static, Button
from textual.reactive import reactive
from textual.screen import Screen
from textual.binding import Binding
from textual.message import Message
from rich.text import Text

from models import NodeConfig, APIKeys, SSHConfig
from widgets.section_tree import SectionTree
from widgets.field_editor import FieldEditor, FieldUpdated
from widgets.validation_display import ValidationDisplay, ValidationItem
from utils.backup import BackupManager
from utils.config_loader import ConfigLoader
from validation.engine import ValidationEngine


class ConfigFile:
    """Represents a configuration file type."""
    
    def __init__(self, name: str, filename: str, model_class):
        self.name = name
        self.filename = filename
        self.model_class = model_class


class ConfigManager(App):
    """Main configuration manager application."""
    
    CSS = """
    ConfigManager {
        background: $background;
    }
    
    .header-bar {
        height: 3;
        background: $primary;
        padding: 1;
    }
    
    .main-container {
        height: 100%;
    }
    
    .sidebar {
        width: 25;
        background: $panel;
        padding: 1;
        border-right: solid $primary;
    }
    
    .editor-panel {
        padding: 1;
    }
    
    .validation-panel {
        height: 8;
        background: $panel;
        border-top: solid $primary;
        padding: 1;
    }
    
    .footer-bar {
        height: 3;
        background: $primary;
    }
    
    .section-active {
        background: $accent;
        color: $text;
    }
    
    .field-error {
        color: $error;
    }
    
    .field-valid {
        color: $success;
    }
    """
    
    BINDINGS = [
        Binding("s", "save", "Save", key_display="S"),
        Binding("r", "reset", "Reset", key_display="R"),
        Binding("v", "validate", "Validate", key_display="V"),
        Binding("t", "template", "Template", key_display="T"),
        Binding("q", "quit", "Quit", key_display="Q"),
        Binding("f1", "help", "Help", key_display="F1"),
    ]
    
    # Configuration files
    CONFIG_FILES = [
        ConfigFile("config.toml", "config.toml", NodeConfig),
        ConfigFile("api-keys.json", "api-keys.json", APIKeys),
        ConfigFile("ssh-config.json", "ssh-config.json", SSHConfig),
    ]
    
    def __init__(self, config_dir: Path = Path(".")):
        super().__init__()
        self.config_dir = config_dir
        self.current_file: Optional[ConfigFile] = self.CONFIG_FILES[0]
        self.current_section: Optional[str] = None
        self.current_data: Optional[Dict[str, Any]] = None
        self.modified = False
        
        # Initialize managers
        self.backup_manager = BackupManager(config_dir)
        self.config_loader = ConfigLoader(config_dir)
        self.validation_engine = ValidationEngine()
    
    def compose(self) -> ComposeResult:
        """Create the UI layout."""
        yield Header()
        
        with Container(classes="main-container"):
            # Header bar with file selector and status
            with Horizontal(classes="header-bar"):
                yield Label("File: ", id="file-label")
                yield Button(self.current_file.name, id="file-selector", variant="primary")
                yield Label("Mode: Edit", id="mode-label")
                yield Label("Status: ", id="status-label")
            
            with Horizontal():
                # Sidebar with section tree
                with Vertical(classes="sidebar"):
                    yield Label("Sections", classes="section-header")
                    yield SectionTree(id="section-tree")
                
                # Main editor panel
                with Vertical(classes="editor-panel"):
                    yield ScrollableContainer(
                        FieldEditor(id="field-editor"),
                        id="editor-scroll"
                    )
            
            # Validation panel
            yield ValidationDisplay(classes="validation-panel", id="validation-display")
        
        yield Footer()
    
    async def on_mount(self) -> None:
        """Initialize the application on mount."""
        await self.load_current_file()
        self.update_section_tree()
    
    async def load_current_file(self) -> None:
        """Load the current configuration file."""
        if not self.current_file:
            return
        
        try:
            self.current_data = await self.config_loader.load_file(
                self.current_file.filename,
                self.current_file.model_class
            )
            self.modified = False
            await self.update_status("File loaded successfully", "success")
        except Exception as e:
            await self.update_status(f"Error loading file: {str(e)}", "error")
            self.current_data = {}
    
    def update_section_tree(self) -> None:
        """Update the section tree based on current file."""
        tree = self.query_one("#section-tree", SectionTree)
        tree.clear()
        
        if not self.current_data:
            return
        
        # Build tree based on current data structure
        tree.build_from_data(self.current_data, self.current_file.model_class)
    
    async def update_status(self, message: str, status_type: str = "info") -> None:
        """Update the status label."""
        status_label = self.query_one("#status-label", Label)
        
        # Apply color based on status type
        color_map = {
            "info": "cyan",
            "success": "green",
            "warning": "yellow",
            "error": "red"
        }
        color = color_map.get(status_type, "white")
        
        status_label.update(Text(f"Status: {message}", style=color))
    
    async def action_save(self) -> None:
        """Save the current configuration."""
        if not self.current_data or not self.modified:
            await self.update_status("No changes to save", "info")
            return
        
        try:
            # Create backup first
            backup_path = await self.backup_manager.create_backup(
                self.current_file.filename
            )
            
            # Validate before saving
            validation_result = self.validation_engine.validate_data(
                self.current_data,
                self.current_file.model_class
            )
            
            if not validation_result.is_valid:
                await self.update_status("Cannot save: validation errors", "error")
                return
            
            # Save the file
            await self.config_loader.save_file(
                self.current_file.filename,
                self.current_data,
                self.current_file.model_class
            )
            
            self.modified = False
            await self.update_status(f"Saved successfully (backup: {backup_path.name})", "success")
            
        except Exception as e:
            await self.update_status(f"Save failed: {str(e)}", "error")
    
    async def action_reset(self) -> None:
        """Reset changes to the current file."""
        if not self.modified:
            await self.update_status("No changes to reset", "info")
            return
        
        await self.load_current_file()
        await self.update_status("Changes reset", "success")
    
    async def action_validate(self) -> None:
        """Validate the current configuration."""
        if not self.current_data:
            return
        
        validation_display = self.query_one("#validation-display", ValidationDisplay)
        
        # Use validation engine to validate data
        result = self.validation_engine.validate_data(
            self.current_data,
            self.current_file.model_class
        )
        
        # Convert validation result to display format
        validation_items = []
        for error in result.errors:
            validation_items.append(ValidationItem(
                error["field"], error["message"], "error"
            ))
        for warning in result.warnings:
            validation_items.append(ValidationItem(
                warning["field"], warning["message"], "warning"
            ))
        for info in result.info:
            validation_items.append(ValidationItem(
                info["field"], info["message"], "info"
            ))
        
        validation_display.update_validation(validation_items)
        
        if result.errors:
            await self.update_status(f"{len(result.errors)} validation errors", "error")
        elif result.warnings:
            await self.update_status(f"{len(result.warnings)} warnings", "warning")
        else:
            await self.update_status("Validation passed", "success")
    
    async def action_template(self) -> None:
        """Show template selection screen."""
        # TODO: Implement template selection
        await self.update_status("Template feature coming soon", "info")
    
    async def action_help(self) -> None:
        """Show help screen."""
        # TODO: Implement help screen
        await self.update_status("Help feature coming soon", "info")
    
    def on_section_tree_section_selected(self, message: SectionTree.SectionSelected) -> None:
        """Handle section selection from the tree."""
        self.current_section = message.section_path
        
        # Update the field editor with the selected section
        field_editor = self.query_one("#field-editor", FieldEditor)
        section_data = self.get_section_data(message.section_path)
        
        # Convert Pydantic model to JSON schema for the field editor
        model_schema = self.current_file.model_class.model_json_schema()
        
        # Navigate to the correct schema section
        section_schema = model_schema
        for key in message.section_path:
            if "properties" in section_schema and key in section_schema["properties"]:
                section_schema = section_schema["properties"][key]
        
        field_editor.update_content(section_data, section_schema, "/".join(message.section_path))
    
    def get_section_data(self, section_path: List[str]) -> Dict[str, Any]:
        """Get data for a specific section path."""
        data = self.current_data
        for key in section_path:
            if isinstance(data, dict) and key in data:
                data = data[key]
            else:
                return {}
        return data if isinstance(data, dict) else {}
    
    def on_field_editor_field_updated(self, message: FieldUpdated) -> None:
        """Handle field changes from the editor."""
        self.modified = True
        
        # Update the data structure
        if self.current_section:
            full_path = self.current_section + [message.field_path]
        else:
            full_path = [message.field_path]
        
        self.update_field_value(full_path, message.value)
        
        # Run validation
        self.action_validate()
    
    def update_field_value(self, field_path: List[str], value: Any) -> None:
        """Update a field value in the data structure."""
        data = self.current_data
        for key in field_path[:-1]:
            if key not in data:
                data[key] = {}
            data = data[key]
        
        data[field_path[-1]] = value
