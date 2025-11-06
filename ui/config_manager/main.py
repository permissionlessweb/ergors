#!/usr/bin/env python3
"""Entry point for the CW-AGENT configuration manager."""

import sys
from pathlib import Path
import click
from app import ConfigManager


@click.command()
@click.option(
    '--config-dir',
    '-d',
    type=click.Path(exists=True, file_okay=False, dir_okay=True, path_type=Path),
    default='.',
    help='Directory containing configuration files'
)
@click.option(
    '--debug',
    is_flag=True,
    help='Run in debug mode'
)
def main(config_dir: Path, debug: bool):
    """CW-AGENT Configuration Manager - A TUI for managing node configurations."""
    app = ConfigManager(config_dir=config_dir)
    
    if debug:
        app.run(debug=True)
    else:
        app.run()


if __name__ == "__main__":
    main()
