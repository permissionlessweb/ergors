"""Section tree widget for navigating configuration sections."""

from typing import Dict, Any, List, Optional, Type
from textual.widgets import Tree
from textual.message import Message
from pydantic import BaseModel
from pydantic.fields import FieldInfo


class SectionTree(Tree):
    """Tree widget for configuration sections."""
    
    class SectionSelected(Message):
        """Message sent when a section is selected."""
        
        def __init__(self, section_path: List[str]) -> None:
            super().__init__()
            self.section_path = section_path
    
    def __init__(self, *args, **kwargs):
        super().__init__("Configuration", *args, **kwargs)
        self.section_paths: Dict[int, List[str]] = {}
    
    def build_from_data(self, data: Dict[str, Any], model_class: Type[BaseModel]) -> None:
        """Build tree from configuration data and model."""
        self.clear()
        self.section_paths.clear()
        
        root = self.root
        root.expand()
        
        # Build tree structure based on model fields
        if hasattr(model_class, '__fields__'):
            for field_name, field in model_class.__fields__.items():
                if field_name in data:
                    self._add_section_node(root, field_name, field, data[field_name], [field_name])
    
    def _add_section_node(
        self,
        parent_node,
        name: str,
        field: Optional[FieldInfo],
        value: Any,
        path: List[str]
    ) -> None:
        """Add a section node to the tree."""
        # Create display name
        display_name = name.replace('_', ' ').title()
        
        # Add type indicator
        if isinstance(value, dict):
            icon = "📁"
        elif isinstance(value, list):
            icon = "📋"
            display_name += f" [{len(value)}]"
        else:
            icon = "📄"
        
        node = parent_node.add(f"{icon} {display_name}")
        node_id = id(node)
        self.section_paths[node_id] = path.copy()
        
        # Handle nested structures
        if isinstance(value, dict) and field:
            # Try to get nested model type
            nested_model = self._get_nested_model_type(field)
            if nested_model and hasattr(nested_model, '__fields__'):
                for sub_name, sub_field in nested_model.__fields__.items():
                    if sub_name in value:
                        sub_path = path + [sub_name]
                        self._add_section_node(
                            node,
                            sub_name,
                            sub_field,
                            value[sub_name],
                            sub_path
                        )
            else:
                # Just add dict keys
                for key, val in value.items():
                    sub_path = path + [key]
                    self._add_section_node(node, key, None, val, sub_path)
        
        elif isinstance(value, list) and len(value) > 0:
            # Add list items
            for i, item in enumerate(value):
                item_name = f"Item {i}"
                if isinstance(item, dict) and 'name' in item:
                    item_name = item['name']
                
                sub_path = path + [str(i)]
                self._add_section_node(node, item_name, None, item, sub_path)
    
    def _get_nested_model_type(self, field: FieldInfo) -> Optional[Type[BaseModel]]:
        """Extract nested model type from field."""
        # In Pydantic v2, use annotation instead of type_
        field_type = getattr(field, 'annotation', None)
        if field_type is None:
            return None
            
        # Handle Optional types
        if hasattr(field_type, '__origin__'):
            import types
            if field_type.__origin__ is types.UnionType or str(field_type.__origin__) == 'typing.Union':
                args = getattr(field_type, '__args__', ())
                # Filter out NoneType for Optional
                field_type = next((arg for arg in args if arg is not type(None)), None)
                if field_type is None:
                    return None
        
        # Check if it's a BaseModel subclass
        if isinstance(field_type, type) and issubclass(field_type, BaseModel):
            return field_type
        
        return None
    
    def on_tree_node_highlighted(self, event: Tree.NodeHighlighted) -> None:
        """Handle node selection."""
        node_id = id(event.node)
        if node_id in self.section_paths:
            path = self.section_paths[node_id]
            self.post_message(self.SectionSelected(path))
