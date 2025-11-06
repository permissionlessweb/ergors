"""Field editor widget for editing configuration values."""

from typing import Any, Optional, Dict, List, Union
from textual.app import ComposeResult
from textual.containers import VerticalScroll, Horizontal, Container
from textual.widgets import Static, Input, Switch, Label, Button
from textual.widget import Widget
from textual.reactive import reactive
from textual.message import Message
from pydantic import BaseModel
from pydantic.fields import FieldInfo


class FieldUpdated(Message):
    """Message emitted when a field is updated."""
    
    def __init__(self, field_path: str, value: Any) -> None:
        super().__init__()
        self.field_path = field_path
        self.value = value


class FieldEditor(Widget):
    """Dynamic form editor for configuration fields."""
    
    DEFAULT_CSS = """
    FieldEditor {
        height: 100%;
        width: 100%;
    }
    
    .field-container {
        padding: 1;
        margin-bottom: 1;
    }
    
    .field-label {
        width: 30%;
        margin-right: 2;
    }
    
    .field-input {
        width: 65%;
    }
    
    .field-description {
        margin-top: 1;
        color: $text-muted;
        text-style: italic;
    }
    
    .array-controls {
        margin-top: 1;
    }
    
    .array-item {
        margin-bottom: 1;
        padding: 1;
        border: solid $primary;
    }
    
    .required-field {
        color: $warning;
    }
    """
    
    current_data = reactive({})
    current_schema = reactive({})
    current_path = reactive("")
    
    def compose(self) -> ComposeResult:
        """Create the field editor layout."""
        yield VerticalScroll(id="field-scroll")
    
    def update_content(self, data: Dict[str, Any], schema: Dict[str, Any], path: str = "") -> None:
        """Update the editor with new data and schema."""
        self.current_data = data
        self.current_schema = schema
        self.current_path = path
        self._rebuild_fields()
    
    def _rebuild_fields(self) -> None:
        """Rebuild all field widgets based on current data and schema."""
        scroll = self.query_one("#field-scroll", VerticalScroll)
        scroll.remove_children()
        
        if not self.current_schema:
            scroll.mount(Static("No schema available"))
            return
        
        # Get the properties from the schema
        properties = self.current_schema.get("properties", {})
        required_fields = self.current_schema.get("required", [])
        
        # Create field widgets
        for field_name, field_schema in properties.items():
            field_value = self.current_data.get(field_name)
            field_widget = self._create_field_widget(
                field_name, 
                field_schema, 
                field_value,
                field_name in required_fields
            )
            if field_widget:
                scroll.mount(field_widget)
    
    def _create_field_widget(self, name: str, schema: Dict[str, Any], value: Any, required: bool) -> Optional[Widget]:
        """Create appropriate widget based on field type."""
        field_type = schema.get("type", "string")
        description = schema.get("description", "")
        
        container = Container(classes="field-container")
        
        # Label
        label_text = name.replace("_", " ").title()
        if required:
            label_text += " *"
        label = Label(label_text, classes="field-label")
        if required:
            label.add_class("required-field")
        
        # Create input based on type
        if field_type == "string":
            if "enum" in schema:
                # TODO: Implement dropdown for enum values
                input_widget = self._create_enum_input(name, schema["enum"], value)
            else:
                input_widget = Input(
                    value=str(value or ""),
                    id=f"field_{name}",
                    classes="field-input"
                )
                input_widget.field_name = name
        
        elif field_type == "integer" or field_type == "number":
            input_widget = Input(
                value=str(value or ""),
                id=f"field_{name}",
                classes="field-input",
                type="number"
            )
            input_widget.field_name = name
        
        elif field_type == "boolean":
            input_widget = Switch(
                value=bool(value),
                id=f"field_{name}",
                classes="field-input"
            )
            input_widget.field_name = name
        
        elif field_type == "array":
            return self._create_array_widget(name, schema, value or [], required)
        
        elif field_type == "object":
            # TODO: Handle nested objects
            return Static(f"Object field: {name} (not yet implemented)")
        
        else:
            return Static(f"Unsupported field type: {field_type}")
        
        # Build the container
        with container:
            with Horizontal():
                container.mount(label)
                container.mount(input_widget)
            
            if description:
                container.mount(Static(description, classes="field-description"))
        
        return container
    
    def _create_enum_input(self, name: str, options: List[str], value: Any) -> Widget:
        """Create a dropdown-like input for enum values."""
        # For now, using a simple Input with validation
        # TODO: Replace with proper dropdown widget
        input_widget = Input(
            value=str(value or options[0] if options else ""),
            id=f"field_{name}",
            classes="field-input",
            placeholder=f"Options: {', '.join(options)}"
        )
        input_widget.field_name = name
        input_widget.enum_options = options
        return input_widget
    
    def _create_array_widget(self, name: str, schema: Dict[str, Any], values: List[Any], required: bool) -> Widget:
        """Create widget for array editing."""
        container = Container(classes="field-container")
        
        label_text = name.replace("_", " ").title()
        if required:
            label_text += " *"
        
        with container:
            label = Label(label_text, classes="field-label")
            if required:
                label.add_class("required-field")
            container.mount(label)
            
            # Array items container
            array_container = Container(id=f"array_{name}", classes="array-container")
            
            # Add existing items
            for i, item in enumerate(values):
                item_widget = self._create_array_item(name, i, item, schema.get("items", {}))
                array_container.mount(item_widget)
            
            container.mount(array_container)
            
            # Add/Remove buttons
            with container:
                controls = Horizontal(classes="array-controls")
                controls.mount(Button("Add Item", id=f"add_{name}", variant="primary"))
                controls.mount(Button("Remove Last", id=f"remove_{name}", variant="warning"))
                container.mount(controls)
        
        return container
    
    def _create_array_item(self, array_name: str, index: int, value: Any, item_schema: Dict[str, Any]) -> Widget:
        """Create widget for a single array item."""
        item_type = item_schema.get("type", "string")
        
        container = Container(classes="array-item")
        with container:
            container.mount(Label(f"Item {index + 1}"))
            
            if item_type == "string":
                input_widget = Input(
                    value=str(value or ""),
                    id=f"field_{array_name}_{index}",
                    classes="field-input"
                )
            elif item_type in ["integer", "number"]:
                input_widget = Input(
                    value=str(value or ""),
                    id=f"field_{array_name}_{index}",
                    classes="field-input",
                    type="number"
                )
            else:
                input_widget = Static(f"Unsupported array item type: {item_type}")
            
            container.mount(input_widget)
        
        return container
    
    async def on_input_changed(self, event: Input.Changed) -> None:
        """Handle input changes."""
        if hasattr(event.input, "field_name"):
            field_name = event.input.field_name
            value = event.value
            
            # Type conversion based on current schema
            if field_name in self.current_schema.get("properties", {}):
                field_schema = self.current_schema["properties"][field_name]
                field_type = field_schema.get("type")
                
                if field_type == "integer":
                    try:
                        value = int(value) if value else None
                    except ValueError:
                        return  # Invalid input
                elif field_type == "number":
                    try:
                        value = float(value) if value else None
                    except ValueError:
                        return  # Invalid input
            
            # Update data and emit message
            self.current_data[field_name] = value
            self.post_message(FieldUpdated(field_name, value))
    
    async def on_switch_changed(self, event: Switch.Changed) -> None:
        """Handle switch changes."""
        if hasattr(event.switch, "field_name"):
            field_name = event.switch.field_name
            self.current_data[field_name] = event.value
            self.post_message(FieldUpdated(field_name, event.value))
    
    async def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses for array management."""
        button_id = event.button.id
        
        if button_id and button_id.startswith("add_"):
            array_name = button_id[4:]  # Remove "add_" prefix
            await self._add_array_item(array_name)
        
        elif button_id and button_id.startswith("remove_"):
            array_name = button_id[7:]  # Remove "remove_" prefix
            await self._remove_array_item(array_name)
    
    async def _add_array_item(self, array_name: str) -> None:
        """Add a new item to an array field."""
        if array_name not in self.current_data:
            self.current_data[array_name] = []
        
        # Get item schema
        field_schema = self.current_schema.get("properties", {}).get(array_name, {})
        item_schema = field_schema.get("items", {})
        item_type = item_schema.get("type", "string")
        
        # Add default value based on type
        if item_type == "string":
            self.current_data[array_name].append("")
        elif item_type in ["integer", "number"]:
            self.current_data[array_name].append(0)
        elif item_type == "boolean":
            self.current_data[array_name].append(False)
        else:
            self.current_data[array_name].append(None)
        
        # Rebuild the array widget
        self._rebuild_fields()
        self.post_message(FieldUpdated(array_name, self.current_data[array_name]))
    
    async def _remove_array_item(self, array_name: str) -> None:
        """Remove the last item from an array field."""
        if array_name in self.current_data and self.current_data[array_name]:
            self.current_data[array_name].pop()
            self._rebuild_fields()
            self.post_message(FieldUpdated(array_name, self.current_data[array_name]))
