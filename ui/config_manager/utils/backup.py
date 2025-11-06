"""Backup management utilities for configuration files."""

from pathlib import Path
from datetime import datetime
import shutil
import asyncio
from typing import List, Optional


class BackupManager:
    """Manages backups of configuration files."""
    
    def __init__(self, config_dir: Path, backup_dir: Optional[Path] = None):
        self.config_dir = Path(config_dir)
        self.backup_dir = backup_dir or (self.config_dir / '.backups')
        self.backup_dir.mkdir(exist_ok=True)
    
    async def create_backup(self, filename: str) -> Path:
        """Create a backup of a configuration file."""
        source_path = self.config_dir / filename
        
        if not source_path.exists():
            raise FileNotFoundError(f"Source file {filename} not found")
        
        # Generate backup filename with timestamp
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        backup_name = f"{source_path.stem}_{timestamp}{source_path.suffix}"
        backup_path = self.backup_dir / backup_name
        
        # Copy file asynchronously
        def _copy():
            shutil.copy2(source_path, backup_path)
        
        loop = asyncio.get_event_loop()
        await loop.run_in_executor(None, _copy)
        
        # Clean old backups
        await self._cleanup_old_backups(filename)
        
        return backup_path
    
    async def restore_backup(self, backup_filename: str, target_filename: str) -> None:
        """Restore a configuration from backup."""
        backup_path = self.backup_dir / backup_filename
        target_path = self.config_dir / target_filename
        
        if not backup_path.exists():
            raise FileNotFoundError(f"Backup file {backup_filename} not found")
        
        # Create backup of current file before restoring
        if target_path.exists():
            await self.create_backup(target_filename)
        
        # Restore from backup
        def _copy():
            shutil.copy2(backup_path, target_path)
        
        loop = asyncio.get_event_loop()
        await loop.run_in_executor(None, _copy)
    
    async def list_backups(self, filename: Optional[str] = None) -> List[Path]:
        """List all backups, optionally filtered by original filename."""
        def _list():
            backups = []
            for backup_file in self.backup_dir.iterdir():
                if backup_file.is_file():
                    if filename:
                        # Filter by original filename
                        base_name = Path(filename).stem
                        if backup_file.name.startswith(base_name):
                            backups.append(backup_file)
                    else:
                        backups.append(backup_file)
            
            # Sort by modification time (newest first)
            return sorted(backups, key=lambda p: p.stat().st_mtime, reverse=True)
        
        loop = asyncio.get_event_loop()
        return await loop.run_in_executor(None, _list)
    
    async def delete_backup(self, backup_filename: str) -> None:
        """Delete a specific backup file."""
        backup_path = self.backup_dir / backup_filename
        
        if not backup_path.exists():
            raise FileNotFoundError(f"Backup file {backup_filename} not found")
        
        def _delete():
            backup_path.unlink()
        
        loop = asyncio.get_event_loop()
        await loop.run_in_executor(None, _delete)
    
    async def _cleanup_old_backups(self, filename: str, max_backups: int = 10) -> None:
        """Remove old backups, keeping only the most recent ones."""
        backups = await self.list_backups(filename)
        
        if len(backups) > max_backups:
            # Delete oldest backups
            for backup_path in backups[max_backups:]:
                await self.delete_backup(backup_path.name)
    
    def get_backup_info(self, backup_path: Path) -> dict:
        """Get information about a backup file."""
        stat = backup_path.stat()
        
        # Parse timestamp from filename
        parts = backup_path.stem.split('_')
        if len(parts) >= 3:
            date_str = parts[-2]
            time_str = parts[-1]
            try:
                timestamp = datetime.strptime(f"{date_str}_{time_str}", "%Y%m%d_%H%M%S")
            except ValueError:
                timestamp = datetime.fromtimestamp(stat.st_mtime)
        else:
            timestamp = datetime.fromtimestamp(stat.st_mtime)
        
        return {
            'filename': backup_path.name,
            'path': backup_path,
            'size': stat.st_size,
            'created': timestamp,
            'original_file': self._get_original_filename(backup_path)
        }
    
    def _get_original_filename(self, backup_path: Path) -> str:
        """Extract original filename from backup filename."""
        # Remove timestamp suffix to get original name
        parts = backup_path.stem.split('_')[:-2]
        if parts:
            return '_'.join(parts) + backup_path.suffix
        return backup_path.name
