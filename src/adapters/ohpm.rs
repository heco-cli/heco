use crate::command::CommandRunner;
use crate::config::Config;
use anstream::println;
use owo_colors::OwoColorize;
use std::path::Path;

pub fn install(project_root: &Path, config: &Config, quiet: bool) -> anyhow::Result<()> {
    let ohpm_path = config
        .ohpm_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 ohpm 路径"))?;

    let sdk_path = config
        .sdk_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 SDK 路径"))?;

    let node_path = config
        .node_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 Node 路径"))?;

    let node_bin = node_path.parent().unwrap();
    let current_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", node_bin.to_str().unwrap_or(""), current_path);

    let runner = CommandRunner::new(project_root.to_path_buf())
        .env("DEVECO_SDK_HOME", sdk_path.to_str().unwrap_or(""))
        .env("PATH", &new_path);

    let program_args = vec!["install", "--all"];

    let ohpm_path_str = ohpm_path.to_str().unwrap_or("ohpm");

    if quiet {
        let output = runner.run_captured_merged(ohpm_path_str, &program_args)?;
        if !output.status.success() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
            anyhow::bail!("{}", "error: ohpm install failed".red());
        }
    } else {
        println!("{:>9} dependencies", "Installing".green().bold());
        runner.run_with_handler(ohpm_path_str, &program_args, |line| {
            println!("  {}", line);
        })?;
    }

    Ok(())
}
