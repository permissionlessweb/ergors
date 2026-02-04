//! Build script to setup Python virtualenv for REPL workers.

use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

fn main() {
    println!("cargo:rerun-if-changed=python/");

    // Check if Python 3 is available
    let python_check = Command::new("python3").arg("--version").output();

    match python_check {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            println!("cargo:warning=Found Python: {}", version.trim());
        }
        _ => {
            println!("cargo:warning=Python 3 not found. RLM service will not work.");
            println!("cargo:warning=Please install Python 3.8+ to use the RLM service.");
            return;
        }
    }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();
    let venv_dir = PathBuf::from(&out_dir).join("venv");
    let requirements_path = PathBuf::from(&manifest_dir).join("python/requirements.txt");

    // Create venv if it doesn't exist
    if !venv_dir.exists() {
        println!("cargo:warning=Creating Python venv at {:?}...", venv_dir);

        let create_venv = Command::new("python3")
            .args(["-m", "venv", venv_dir.to_str().unwrap()])
            .status();

        match create_venv {
            Ok(status) if status.success() => {
                println!("cargo:warning=Python venv created successfully");
            }
            Ok(status) => {
                println!(
                    "cargo:warning=Failed to create venv (exit code: {})",
                    status
                );
                println!("cargo:warning=RLM service will not work correctly");
                return;
            }
            Err(e) => {
                println!("cargo:warning=Error creating venv: {}", e);
                println!("cargo:warning=RLM service will not work correctly");
                return;
            }
        }
    }

    // Determine venv python and pip paths
    let (venv_python, venv_pip) = if cfg!(windows) {
        (
            venv_dir.join("Scripts").join("python.exe"),
            venv_dir.join("Scripts").join("pip.exe"),
        )
    } else {
        (
            venv_dir.join("bin").join("python3"),
            venv_dir.join("bin").join("pip"),
        )
    };

    // Install dependencies in venv
    println!(
        "cargo:warning=Installing Python dependencies from {:?}...",
        requirements_path
    );

    let pip_install = Command::new(&venv_pip)
        .args([
            "install",
            "-r",
            requirements_path.to_str().unwrap(),
            "--quiet",
        ])
        .status();

    match pip_install {
        Ok(status) if status.success() => {
            println!("cargo:warning=Python dependencies installed successfully");
        }
        Ok(status) => {
            println!(
                "cargo:warning=Failed to install Python dependencies (exit code: {})",
                status
            );
            println!("cargo:warning=RLM service may not work correctly");
            return;
        }
        Err(e) => {
            println!("cargo:warning=Error installing Python dependencies: {}", e);
            println!("cargo:warning=RLM service may not work correctly");
            return;
        }
    }

    // Verify RestrictedPython is installed in venv
    let verify = Command::new(&venv_python)
        .args(["-c", "import RestrictedPython; print('OK')"])
        .output();

    match verify {
        Ok(output) if output.status.success() => {
            println!("cargo:warning=RestrictedPython verified in venv");
        }
        _ => {
            println!(
                "cargo:warning=RestrictedPython not available in venv. RLM service will not work."
            );
            return;
        }
    }

    // Write venv python path to a file for runtime use
    let venv_path_file = PathBuf::from(&manifest_dir).join("target").join("venv_python_path");
    fs::create_dir_all(venv_path_file.parent().unwrap()).ok();
    fs::write(&venv_path_file, venv_python.to_str().unwrap()).ok();

    println!(
        "cargo:warning=RLM Python environment ready at {:?}",
        venv_python
    );
}
