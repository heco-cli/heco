use crate::config::Config;
use anyhow::{Result, bail};
use std::path::PathBuf;
use std::process::Command;

pub fn find_hdc_binary(config: &Config) -> Result<PathBuf> {
    if let Some(hdc_path) = config.hdc_path() {
        return Ok(hdc_path);
    }

    bail!(
        "Could not find hdc binary. Please ensure DevEco Studio is installed and configured in heco env."
    )
}

pub fn list_targets(config: &Config) -> Result<Vec<(String, String)>> {
    let hdc_cmd = find_hdc_binary(config)?;
    let mut cmd = Command::new(&hdc_cmd);
    cmd.arg("list").arg("targets");

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to list devices: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    let mut devices = Vec::new();

    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with("[Empty]") {
            continue;
        }

        let target = line.split_whitespace().next().unwrap_or(line).to_string();
        let name = get_device_name(&hdc_cmd, &target);
        devices.push((name, target));
    }

    Ok(devices)
}

pub fn get_device_name(hdc_cmd: &PathBuf, target: &str) -> String {
    // Try to get emulator name first
    if let Ok(output) = Command::new(hdc_cmd)
        .arg("-t")
        .arg(target)
        .arg("shell")
        .arg("param")
        .arg("get")
        .arg("ohos.qemu.hvd.name")
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stdout.is_empty() && !stdout.contains("fail!") && !stdout.contains("not found") {
            return stdout;
        }
    }

    // Try to get physical device name
    if let Ok(output) = Command::new(hdc_cmd)
        .arg("-t")
        .arg(target)
        .arg("shell")
        .arg("param")
        .arg("get")
        .arg("const.product.name")
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stdout.is_empty() && !stdout.contains("fail!") && !stdout.contains("not found") {
            // For emulator, const.product.name returns "emulator" which is less descriptive
            // but for physical devices it might be useful
            if stdout != "emulator" {
                return stdout;
            }
        }
    }

    // Fallback to model if name is just "emulator" or not found
    if let Ok(output) = Command::new(hdc_cmd)
        .arg("-t")
        .arg(target)
        .arg("shell")
        .arg("param")
        .arg("get")
        .arg("const.product.model")
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stdout.is_empty() && !stdout.contains("fail!") && !stdout.contains("not found") {
            return stdout;
        }
    }

    "Unknown Device".to_string()
}
