use anyhow::{Result, anyhow};
use clap::Parser;
use clap_complete::Shell;
use clap_complete::engine::CompletionCandidate;
use clap_complete::env::{Bash, Elvish, EnvCompleter, Fish, Powershell, Zsh};
use std::ffi::OsStr;

// Dynamic completer for modules
pub fn complete_modules(_current: &OsStr) -> Vec<CompletionCandidate> {
    let mut candidates = Vec::new();
    if let Ok(project) = crate::project::load_project() {
        for module in project.modules {
            let help = format!("Module: {} ({})", module.name, module.src_path);
            candidates.push(CompletionCandidate::new(module.name).help(Some(help.into())));
        }
    }
    candidates
}

// Dynamic completer for runnable modules (entry or feature)
pub fn complete_runnable_modules(_current: &OsStr) -> Vec<CompletionCandidate> {
    let mut candidates = Vec::new();
    if let Ok(project) = crate::project::load_project() {
        for module in project.modules {
            if matches!(
                module.module_type,
                crate::project::ModuleType::Entry | crate::project::ModuleType::Feature
            ) {
                let help = format!("Runnable Module: {} ({})", module.name, module.src_path);
                candidates.push(CompletionCandidate::new(module.name).help(Some(help.into())));
            }
        }
    }
    candidates
}

// Dynamic completer for products
pub fn complete_products(_current: &OsStr) -> Vec<CompletionCandidate> {
    let mut candidates = Vec::new();
    if let Ok(project) = crate::project::load_project() {
        for product in project.products {
            candidates.push(CompletionCandidate::new(product));
        }
    }
    candidates
}

// Dynamic completer for devices
pub fn complete_devices(current: &OsStr) -> Vec<CompletionCandidate> {
    let mut candidates = Vec::new();

    // Parse the current input to handle comma-separated values
    let current_str = current.to_string_lossy();
    let (prefix, _last_part) = if let Some(last_comma_idx) = current_str.rfind(',') {
        // e.g. "device1,dev" -> prefix="device1,", last_part="dev"
        let p = &current_str[..=last_comma_idx];
        let l = &current_str[last_comma_idx + 1..];
        (p.to_string(), l.to_string())
    } else {
        (String::new(), current_str.to_string())
    };

    if let Ok(project) = crate::project::load_project()
        && let Ok(config) = crate::config::Config::load(Some(&project.root))
        && let Ok(devices) = crate::adapters::hdc::list_targets(&config)
    {
        for (name, id) in devices {
            // Append the new device name to the existing prefix
            let completion_value = format!("{}{}", prefix, name);
            candidates.push(CompletionCandidate::new(completion_value).help(Some(id.into())));
        }
    }
    candidates
}

// Dynamic completer for emulators
pub fn complete_emulators(_current: &OsStr) -> Vec<CompletionCandidate> {
    let mut candidates = Vec::new();
    if let Ok(emulators) = crate::emulator::get_emulator_list() {
        for name in emulators {
            candidates.push(CompletionCandidate::new(name));
        }
    }
    candidates
}

#[derive(Parser, Debug)]
pub struct CompletionArgs {
    /// Target shell to generate completions for. If omitted, instructions are shown.
    #[arg(value_enum)]
    pub shell: Option<Shell>,
}

pub fn handle_completion(args: CompletionArgs) -> Result<()> {
    match args.shell {
        Some(shell) => {
            let completer: Box<dyn EnvCompleter> = match shell {
                Shell::Bash => Box::new(Bash),
                Shell::Zsh => Box::new(Zsh),
                Shell::Fish => Box::new(Fish),
                Shell::PowerShell => Box::new(Powershell),
                Shell::Elvish => Box::new(Elvish),
                _ => return Err(anyhow!("Unsupported shell: {}", shell)),
            };

            // Generate the dynamic registration script
            if let Err(e) = completer.write_registration(
                "COMPLETE",
                "heco",
                "heco",
                "heco", // completer is the CLI binary itself
                &mut std::io::stdout(),
            ) && e.kind() != std::io::ErrorKind::BrokenPipe
            {
                return Err(e.into());
            }
        }
        None => {
            // Print user-friendly configuration guide
            let current_shell = std::env::var("SHELL").unwrap_or_default();
            let mut detected = "";

            if current_shell.ends_with("zsh") {
                detected = "zsh";
            } else if current_shell.ends_with("bash") {
                detected = "bash";
            } else if current_shell.ends_with("fish") {
                detected = "fish";
            }

            println!("heco command-line completions (Dynamic)");
            println!("=======================================");
            println!(
                "To enable dynamic completions for your shell, run the corresponding command:"
            );
            println!();

            if detected == "zsh" {
                println!("🌟 Detected Zsh. Add this to your ~/.zshrc:");
                println!("   autoload -Uz compinit; compinit");
                println!("   source <(heco completion zsh)");
            } else {
                println!("Zsh:");
                println!("   autoload -Uz compinit; compinit");
                println!("   source <(heco completion zsh)");
            }
            println!();

            if detected == "bash" {
                println!("🌟 Detected Bash. Add this to your ~/.bashrc:");
                println!("   eval \"$(heco completion bash)\"");
            } else {
                println!("Bash:");
                println!("   eval \"$(heco completion bash)\"");
            }
            println!();

            if detected == "fish" {
                println!("🌟 Detected Fish. Add this to your ~/.config/fish/config.fish:");
                println!("   heco completion fish | source");
            } else {
                println!("Fish:");
                println!("   heco completion fish | source");
            }
            println!();

            println!("PowerShell:");
            println!("   heco completion powershell | Out-String | Invoke-Expression");
            println!();
            println!("(Note: After adding the command to your config, restart your terminal.)");
        }
    }

    Ok(())
}
