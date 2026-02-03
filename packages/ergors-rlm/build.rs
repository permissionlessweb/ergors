//! Build script to setup Python virtualenv for REPL workers.

use std::process::Command;
use std::env;

fn main() {
    println!("cargo:rerun-if-changed=python/");

    // Check if Python 3 is available
    let python_check = Command::new("python3")
        .arg("--version")
        .output();

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

    // Check if pip is available
    let pip_check = Command::new("python3")
        .args(["-m", "pip", "--version"])
        .output();

    if pip_check.is_err() || !pip_check.unwrap().status.success() {
        println!("cargo:warning=pip not found. Installing pip...");

        let install_pip = Command::new("python3")
            .args(["-m", "ensurepip", "--default-pip"])
            .status();

        if install_pip.is_err() || !install_pip.unwrap().success() {
            println!("cargo:warning=Failed to install pip. RLM service may not work.");
            return;
        }
    }

    // Install Python dependencies
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let requirements_path = format!("{}/python/requirements.txt", manifest_dir);

    println!("cargo:warning=Installing Python dependencies from {}...", requirements_path);

    let pip_install = Command::new("python3")
        .args(["-m", "pip", "install", "-r", &requirements_path, "--user", "--quiet"])
        .status();

    match pip_install {
        Ok(status) if status.success() => {
            println!("cargo:warning=Python dependencies installed successfully");
        }
        Ok(status) => {
            println!("cargo:warning=Failed to install Python dependencies (exit code: {})", status);
            println!("cargo:warning=RLM service may not work correctly");
        }
        Err(e) => {
            println!("cargo:warning=Error installing Python dependencies: {}", e);
            println!("cargo:warning=RLM service may not work correctly");
        }
    }

    // Verify RestrictedPython is installed
    let verify = Command::new("python3")
        .args(["-c", "import RestrictedPython; print('OK')"])
        .output();

    match verify {
        Ok(output) if output.status.success() => {
            println!("cargo:warning=RestrictedPython verified");
        }
        _ => {
            println!("cargo:warning=RestrictedPython not available. RLM service will not work.");
        }
    }
}
