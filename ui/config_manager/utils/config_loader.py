"""Configuration file loading and saving utilities."""

from pathlib import Path
from typing import Dict, Any, Type, Optional
import toml
import json
from pydantic import BaseModel, ValidationError
import asyncio


class ConfigLoader:
    """Handles loading and saving configuration files."""
    
    def __init__(self, config_dir: Path):
        self.config_dir = Path(config_dir)
    
    async def load_file(
        self,
        filename: str,
        model_class: Type[BaseModel]
    ) -> Dict[str, Any]:
        """Load a configuration file and optionally validate with model."""
        file_path = self.config_dir / filename
        
        if not file_path.exists():
            # Return empty dict for new files
            return {}
        
        # Load based on file extension
        if filename.endswith('.toml'):
            return await self._load_toml(file_path)
        elif filename.endswith('.json'):
            return await self._load_json(file_path)
        else:
            raise ValueError(f"Unsupported file type: {filename}")
    
    async def save_file(
        self,
        filename: str,
        data: Dict[str, Any],
        model_class: Type[BaseModel]
    ) -> None:
        """Save configuration data to file."""
        file_path = self.config_dir / filename
        
        # Validate data with model if provided
        if model_class:
            try:
                model_class(**data)
            except ValidationError as e:
                raise ValueError(f"Validation error: {e}")
        
        # Save based on file extension
        if filename.endswith('.toml'):
            await self._save_toml(file_path, data)
        elif filename.endswith('.json'):
            await self._save_json(file_path, data)
        else:
            raise ValueError(f"Unsupported file type: {filename}")
    
    async def _load_toml(self, file_path: Path) -> Dict[str, Any]:
        """Load TOML file."""
        def _load():
            with open(file_path, 'r') as f:
                return toml.load(f)
        
        loop = asyncio.get_event_loop()
        return await loop.run_in_executor(None, _load)
    
    async def _load_json(self, file_path: Path) -> Dict[str, Any]:
        """Load JSON file."""
        def _load():
            with open(file_path, 'r') as f:
                return json.load(f)
        
        loop = asyncio.get_event_loop()
        return await loop.run_in_executor(None, _load)
    
    async def _save_toml(self, file_path: Path, data: Dict[str, Any]) -> None:
        """Save TOML file."""
        def _save():
            # Convert data to TOML-compatible format
            toml_data = self._prepare_for_toml(data)
            with open(file_path, 'w') as f:
                toml.dump(toml_data, f)
        
        loop = asyncio.get_event_loop()
        await loop.run_in_executor(None, _save)
    
    async def _save_json(self, file_path: Path, data: Dict[str, Any]) -> None:
        """Save JSON file."""
        def _save():
            with open(file_path, 'w') as f:
                json.dump(data, f, indent=2)
        
        loop = asyncio.get_event_loop()
        await loop.run_in_executor(None, _save)
    
    def _prepare_for_toml(self, data: Any) -> Any:
        """Prepare data for TOML serialization."""
        if isinstance(data, dict):
            return {k: self._prepare_for_toml(v) for k, v in data.items()}
        elif isinstance(data, list):
            return [self._prepare_for_toml(item) for item in data]
        elif hasattr(data, 'dict'):  # Pydantic model
            return self._prepare_for_toml(data.dict())
        else:
            return data
    
    def file_exists(self, filename: str) -> bool:
        """Check if a configuration file exists."""
        return (self.config_dir / filename).exists()
    
    def create_default_file(
        self,
        filename: str,
        model_class: Type[BaseModel]
    ) -> Dict[str, Any]:
        """Create a default configuration based on model."""
        # Create instance with defaults
        instance = model_class()
        return instance.dict()
    
    async def validate_file(
        self,
        filename: str,
        model_class: Type[BaseModel]
    ) -> Optional[ValidationError]:
        """Validate a configuration file against its model."""
        try:
            data = await self.load_file(filename, model_class)
            model_class(**data)
            return None
        except ValidationError as e:
            return e
