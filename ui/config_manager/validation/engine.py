"""Validation engine for configuration data."""

from typing import Any, Dict, List, Optional, Union, Type
from pydantic import BaseModel, ValidationError
import json


class ValidationResult:
    """Result of a validation operation."""
    
    def __init__(self):
        self.errors: List[Dict[str, str]] = []
        self.warnings: List[Dict[str, str]] = []
        self.info: List[Dict[str, str]] = []
    
    @property
    def is_valid(self) -> bool:
        """Check if validation passed (no errors)."""
        return len(self.errors) == 0
    
    def add_error(self, field: str, message: str) -> None:
        """Add an error message."""
        self.errors.append({"field": field, "message": message})
    
    def add_warning(self, field: str, message: str) -> None:
        """Add a warning message."""
        self.warnings.append({"field": field, "message": message})
    
    def add_info(self, field: str, message: str) -> None:
        """Add an info message."""
        self.info.append({"field": field, "message": message})
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary format."""
        return {
            "is_valid": self.is_valid,
            "errors": self.errors,
            "warnings": self.warnings,
            "info": self.info
        }


class ValidationEngine:
    """Engine for validating configuration data against schemas."""
    
    def __init__(self):
        self.model_cache: Dict[str, Type[BaseModel]] = {}
    
    def validate_data(self, data: Dict[str, Any], model_class: Type[BaseModel]) -> ValidationResult:
        """Validate data against a Pydantic model."""
        result = ValidationResult()
        
        try:
            # Attempt to create model instance
            instance = model_class(**data)
            
            # If successful, perform additional checks
            self._check_best_practices(data, model_class, result)
            
        except ValidationError as e:
            # Convert Pydantic errors to our format
            for error in e.errors():
                field_path = ".".join(str(x) for x in error["loc"])
                message = error["msg"]
                result.add_error(field_path, message)
        
        return result
    
    def validate_json_schema(self, data: Dict[str, Any], schema: Dict[str, Any]) -> ValidationResult:
        """Validate data against a JSON schema."""
        result = ValidationResult()
        
        # Basic type checking
        properties = schema.get("properties", {})
        required = schema.get("required", [])
        
        # Check required fields
        for field in required:
            if field not in data or data[field] is None:
                result.add_error(field, "This field is required")
        
        # Validate each field
        for field_name, field_value in data.items():
            if field_name in properties:
                field_schema = properties[field_name]
                self._validate_field(field_name, field_value, field_schema, result)
        
        return result
    
    def _validate_field(self, name: str, value: Any, schema: Dict[str, Any], result: ValidationResult) -> None:
        """Validate a single field against its schema."""
        field_type = schema.get("type")
        
        if value is None and "default" not in schema:
            # Check if field is nullable
            if not schema.get("nullable", False):
                result.add_error(name, "Field cannot be null")
            return
        
        # Type validation
        if field_type == "string":
            if not isinstance(value, str):
                result.add_error(name, "Must be a string")
                return
            
            # String constraints
            if "minLength" in schema and len(value) < schema["minLength"]:
                result.add_error(name, f"Must be at least {schema['minLength']} characters")
            
            if "maxLength" in schema and len(value) > schema["maxLength"]:
                result.add_error(name, f"Must be at most {schema['maxLength']} characters")
            
            if "pattern" in schema:
                import re
                if not re.match(schema["pattern"], value):
                    result.add_error(name, f"Does not match required pattern")
            
            if "enum" in schema and value not in schema["enum"]:
                result.add_error(name, f"Must be one of: {', '.join(schema['enum'])}")
        
        elif field_type == "integer":
            if not isinstance(value, int) or isinstance(value, bool):
                result.add_error(name, "Must be an integer")
                return
            
            self._validate_numeric_constraints(name, value, schema, result)
        
        elif field_type == "number":
            if not isinstance(value, (int, float)) or isinstance(value, bool):
                result.add_error(name, "Must be a number")
                return
            
            self._validate_numeric_constraints(name, value, schema, result)
        
        elif field_type == "boolean":
            if not isinstance(value, bool):
                result.add_error(name, "Must be a boolean")
        
        elif field_type == "array":
            if not isinstance(value, list):
                result.add_error(name, "Must be an array")
                return
            
            # Array constraints
            if "minItems" in schema and len(value) < schema["minItems"]:
                result.add_error(name, f"Must have at least {schema['minItems']} items")
            
            if "maxItems" in schema and len(value) > schema["maxItems"]:
                result.add_error(name, f"Must have at most {schema['maxItems']} items")
            
            # Validate array items
            if "items" in schema:
                for i, item in enumerate(value):
                    self._validate_field(f"{name}[{i}]", item, schema["items"], result)
        
        elif field_type == "object":
            if not isinstance(value, dict):
                result.add_error(name, "Must be an object")
                return
            
            # Validate nested object
            if "properties" in schema:
                nested_result = self.validate_json_schema(value, schema)
                for error in nested_result.errors:
                    error["field"] = f"{name}.{error['field']}"
                    result.errors.append(error)
    
    def _validate_numeric_constraints(self, name: str, value: Union[int, float], 
                                    schema: Dict[str, Any], result: ValidationResult) -> None:
        """Validate numeric constraints."""
        if "minimum" in schema and value < schema["minimum"]:
            result.add_error(name, f"Must be at least {schema['minimum']}")
        
        if "maximum" in schema and value > schema["maximum"]:
            result.add_error(name, f"Must be at most {schema['maximum']}")
        
        if "exclusiveMinimum" in schema and value <= schema["exclusiveMinimum"]:
            result.add_error(name, f"Must be greater than {schema['exclusiveMinimum']}")
        
        if "exclusiveMaximum" in schema and value >= schema["exclusiveMaximum"]:
            result.add_error(name, f"Must be less than {schema['exclusiveMaximum']}")
        
        if "multipleOf" in schema and value % schema["multipleOf"] != 0:
            result.add_error(name, f"Must be a multiple of {schema['multipleOf']}")
    
    def _check_best_practices(self, data: Dict[str, Any], model_class: Type[BaseModel], 
                            result: ValidationResult) -> None:
        """Check for configuration best practices."""
        # Golden ratio checks
        if hasattr(model_class, "__name__"):
            model_name = model_class.__name__
            
            if model_name == "NodeConfig":
                self._check_node_config_practices(data, result)
            elif model_name == "LLMConfig":
                self._check_llm_config_practices(data, result)
            elif model_name == "StorageConfig":
                self._check_storage_config_practices(data, result)
    
    def _check_node_config_practices(self, data: Dict[str, Any], result: ValidationResult) -> None:
        """Check Node configuration best practices."""
        # Identity checks
        if "identity" in data:
            identity = data["identity"]
            if "name" in identity and identity["name"]:
                name = identity["name"]
                if len(name) < 3:
                    result.add_warning("identity.name", "Consider using a more descriptive name")
                if not name.replace("_", "").replace("-", "").isalnum():
                    result.add_warning("identity.name", 
                                     "Consider using only letters, numbers, underscores, and hyphens")
        
        # Network checks
        if "network" in data:
            network = data["network"]
            if "rpc_port" in network and network["rpc_port"]:
                port = network["rpc_port"]
                if port < 1024:
                    result.add_warning("network.rpc_port", 
                                     "Ports below 1024 require root privileges")
                if port in [80, 443, 22, 21, 25]:
                    result.add_warning("network.rpc_port", 
                                     "This port is commonly used by other services")
    
    def _check_llm_config_practices(self, data: Dict[str, Any], result: ValidationResult) -> None:
        """Check LLM configuration best practices."""
        if "providers" in data and isinstance(data["providers"], list):
            if len(data["providers"]) == 0:
                result.add_warning("providers", "No LLM providers configured")
            
            # Check for duplicate providers
            provider_names = [p.get("name") for p in data["providers"] if "name" in p]
            if len(provider_names) != len(set(provider_names)):
                result.add_warning("providers", "Duplicate provider names detected")
            
            # Check provider configurations
            for i, provider in enumerate(data["providers"]):
                if "model" in provider and provider["model"]:
                    model = provider["model"]
                    if "gpt-4" in model and "max_tokens" in provider:
                        if provider["max_tokens"] > 8000:
                            result.add_info(f"providers[{i}].max_tokens", 
                                          "GPT-4 supports up to 8,192 tokens")
    
    def _check_storage_config_practices(self, data: Dict[str, Any], result: ValidationResult) -> None:
        """Check Storage configuration best practices."""
        # File system checks
        if "fs" in data and "root_path" in data["fs"]:
            root_path = data["fs"]["root_path"]
            if root_path.startswith("/tmp"):
                result.add_warning("fs.root_path", 
                                 "Temporary directories may be cleared on system restart")
            if root_path == "/" or root_path == "/etc" or root_path == "/var":
                result.add_error("fs.root_path", 
                               "Using system directories is not recommended")
        
        # S3 checks
        if "s3" in data and data["s3"].get("enabled"):
            s3_config = data["s3"]
            if not s3_config.get("bucket"):
                result.add_error("s3.bucket", "S3 bucket name is required when S3 is enabled")
            if not s3_config.get("region"):
                result.add_warning("s3.region", "Consider specifying an AWS region")
    
    def get_field_suggestions(self, field_name: str, current_value: Any, 
                            model_class: Type[BaseModel]) -> List[str]:
        """Get suggestions for a field value."""
        suggestions = []
        
        # Get field info from model
        if hasattr(model_class, "__fields__"):
            fields = model_class.__fields__
            if field_name in fields:
                field_info = fields[field_name]
                
                # Check for enum values
                if hasattr(field_info.annotation, "__args__"):
                    # Handle Optional types
                    for arg in field_info.annotation.__args__:
                        if hasattr(arg, "__members__"):
                            # It's an Enum
                            suggestions.extend([member.value for member in arg])
        
        # Common suggestions based on field name
        if "port" in field_name.lower():
            suggestions.extend(["8080", "3000", "5000", "9000"])
        elif "host" in field_name.lower():
            suggestions.extend(["localhost", "127.0.0.1", "0.0.0.0"])
        elif "region" in field_name.lower():
            suggestions.extend(["us-east-1", "us-west-2", "eu-west-1", "ap-southeast-1"])
        
        return suggestions
