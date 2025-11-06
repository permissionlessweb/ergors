"""Validation display widget for showing validation errors and warnings."""

from typing import List, Dict, Any, Optional
from textual.app import ComposeResult
from textual.containers import VerticalScroll
from textual.widgets import Static, Label
from textual.widget import Widget
from textual.reactive import reactive
from rich.text import Text
from rich.console import Group
from rich.panel import Panel


class ValidationItem:
    """Represents a single validation issue."""
    
    def __init__(self, field: str, message: str, level: str = "error"):
        self.field = field
        self.message = message
        self.level = level  # "error", "warning", "info"
    
    def __str__(self) -> str:
        return f"[{self.level}] {self.field}: {self.message}"


class ValidationDisplay(Widget):
    """Widget to display validation errors and warnings."""
    
    DEFAULT_CSS = """
    ValidationDisplay {
        height: 100%;
        width: 100%;
        background: $surface;
        border: solid $primary;
        padding: 1;
    }
    
    .validation-header {
        text-style: bold;
        margin-bottom: 1;
    }
    
    .validation-error {
        color: $error;
        margin-bottom: 1;
    }
    
    .validation-warning {
        color: $warning;
        margin-bottom: 1;
    }
    
    .validation-info {
        color: $primary;
        margin-bottom: 1;
    }
    
    .validation-success {
        color: $success;
        text-align: center;
        margin-top: 2;
    }
    
    .validation-scroll {
        height: 100%;
    }
    """
    
    validation_items = reactive([])
    is_valid = reactive(True)
    
    def compose(self) -> ComposeResult:
        """Create the validation display layout."""
        yield Label("Validation Results", classes="validation-header")
        yield VerticalScroll(id="validation-scroll", classes="validation-scroll")
    
    def update_validation(self, items: List[ValidationItem]) -> None:
        """Update the validation display with new items."""
        self.validation_items = items
        self.is_valid = not any(item.level == "error" for item in items)
        self._rebuild_display()
    
    def clear_validation(self) -> None:
        """Clear all validation messages."""
        self.validation_items = []
        self.is_valid = True
        self._rebuild_display()
    
    def add_validation(self, field: str, message: str, level: str = "error") -> None:
        """Add a single validation item."""
        item = ValidationItem(field, message, level)
        self.validation_items = list(self.validation_items) + [item]
        self.is_valid = not any(item.level == "error" for item in self.validation_items)
        self._rebuild_display()
    
    def _rebuild_display(self) -> None:
        """Rebuild the validation display."""
        scroll = self.query_one("#validation-scroll", VerticalScroll)
        scroll.remove_children()
        
        if not self.validation_items:
            # Show success message if no issues
            success_msg = Static(
                "✓ All fields are valid",
                classes="validation-success"
            )
            scroll.mount(success_msg)
            return
        
        # Group items by level
        errors = [item for item in self.validation_items if item.level == "error"]
        warnings = [item for item in self.validation_items if item.level == "warning"]
        infos = [item for item in self.validation_items if item.level == "info"]
        
        # Display errors first
        if errors:
            for error in errors:
                error_text = Text()
                error_text.append("✗ ", style="bold red")
                error_text.append(f"{error.field}: ", style="bold")
                error_text.append(error.message)
                
                error_widget = Static(error_text, classes="validation-error")
                scroll.mount(error_widget)
        
        # Then warnings
        if warnings:
            for warning in warnings:
                warning_text = Text()
                warning_text.append("⚠ ", style="bold yellow")
                warning_text.append(f"{warning.field}: ", style="bold")
                warning_text.append(warning.message)
                
                warning_widget = Static(warning_text, classes="validation-warning")
                scroll.mount(warning_widget)
        
        # Finally info messages
        if infos:
            for info in infos:
                info_text = Text()
                info_text.append("ℹ ", style="bold blue")
                info_text.append(f"{info.field}: ", style="bold")
                info_text.append(info.message)
                
                info_widget = Static(info_text, classes="validation-info")
                scroll.mount(info_widget)
    
    def validate_field(self, field_name: str, value: Any, schema: Dict[str, Any]) -> List[ValidationItem]:
        """Validate a single field against its schema."""
        items = []
        field_schema = schema.get("properties", {}).get(field_name, {})
        required_fields = schema.get("required", [])
        
        # Check required
        if field_name in required_fields and (value is None or value == ""):
            items.append(ValidationItem(field_name, "This field is required", "error"))
            return items
        
        # Type validation
        field_type = field_schema.get("type")
        if field_type and value is not None and value != "":
            if field_type == "string":
                if not isinstance(value, str):
                    items.append(ValidationItem(field_name, "Must be a string", "error"))
                
                # Check enum values
                if "enum" in field_schema and value not in field_schema["enum"]:
                    items.append(ValidationItem(
                        field_name, 
                        f"Must be one of: {', '.join(field_schema['enum'])}", 
                        "error"
                    ))
                
                # Check string constraints
                if "minLength" in field_schema and len(value) < field_schema["minLength"]:
                    items.append(ValidationItem(
                        field_name,
                        f"Must be at least {field_schema['minLength']} characters",
                        "error"
                    ))
                
                if "maxLength" in field_schema and len(value) > field_schema["maxLength"]:
                    items.append(ValidationItem(
                        field_name,
                        f"Must be at most {field_schema['maxLength']} characters",
                        "error"
                    ))
                
                # Pattern validation
                if "pattern" in field_schema:
                    import re
                    pattern = field_schema["pattern"]
                    if not re.match(pattern, value):
                        items.append(ValidationItem(
                            field_name,
                            f"Does not match required pattern: {pattern}",
                            "error"
                        ))
            
            elif field_type == "integer":
                try:
                    int_value = int(value)
                    
                    # Check numeric constraints
                    if "minimum" in field_schema and int_value < field_schema["minimum"]:
                        items.append(ValidationItem(
                            field_name,
                            f"Must be at least {field_schema['minimum']}",
                            "error"
                        ))
                    
                    if "maximum" in field_schema and int_value > field_schema["maximum"]:
                        items.append(ValidationItem(
                            field_name,
                            f"Must be at most {field_schema['maximum']}",
                            "error"
                        ))
                except (ValueError, TypeError):
                    items.append(ValidationItem(field_name, "Must be an integer", "error"))
            
            elif field_type == "number":
                try:
                    float_value = float(value)
                    
                    # Check numeric constraints
                    if "minimum" in field_schema and float_value < field_schema["minimum"]:
                        items.append(ValidationItem(
                            field_name,
                            f"Must be at least {field_schema['minimum']}",
                            "error"
                        ))
                    
                    if "maximum" in field_schema and float_value > field_schema["maximum"]:
                        items.append(ValidationItem(
                            field_name,
                            f"Must be at most {field_schema['maximum']}",
                            "error"
                        ))
                except (ValueError, TypeError):
                    items.append(ValidationItem(field_name, "Must be a number", "error"))
            
            elif field_type == "boolean":
                if not isinstance(value, bool):
                    items.append(ValidationItem(field_name, "Must be true or false", "error"))
            
            elif field_type == "array":
                if not isinstance(value, list):
                    items.append(ValidationItem(field_name, "Must be an array", "error"))
                else:
                    # Check array constraints
                    if "minItems" in field_schema and len(value) < field_schema["minItems"]:
                        items.append(ValidationItem(
                            field_name,
                            f"Must have at least {field_schema['minItems']} items",
                            "error"
                        ))
                    
                    if "maxItems" in field_schema and len(value) > field_schema["maxItems"]:
                        items.append(ValidationItem(
                            field_name,
                            f"Must have at most {field_schema['maxItems']} items",
                            "error"
                        ))
        
        # Add warnings for best practices
        if field_name == "name" and value:
            if not value.replace("_", "").replace("-", "").isalnum():
                items.append(ValidationItem(
                    field_name,
                    "Consider using only letters, numbers, underscores, and hyphens",
                    "warning"
                ))
        
        return items
    
    def validate_all(self, data: Dict[str, Any], schema: Dict[str, Any]) -> None:
        """Validate all fields in the data against the schema."""
        all_items = []
        
        # Validate each field
        for field_name, value in data.items():
            items = self.validate_field(field_name, value, schema)
            all_items.extend(items)
        
        # Check for missing required fields
        required_fields = schema.get("required", [])
        for required_field in required_fields:
            if required_field not in data or data[required_field] is None or data[required_field] == "":
                all_items.append(ValidationItem(
                    required_field,
                    "This field is required",
                    "error"
                ))
        
        self.update_validation(all_items)
