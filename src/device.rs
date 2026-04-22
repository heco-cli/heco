use crate::adapters::hdc::list_targets;
use crate::config::Config;
use anstream::println;
use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct DeviceArgs {
    #[command(subcommand)]
    pub command: DeviceCommands,
}

#[derive(Subcommand, Debug)]
pub enum DeviceCommands {
    /// List all active devices
    List(ListArgs),
}

#[derive(Args, Debug)]
pub struct ListArgs {}

pub fn handle_device(args: DeviceArgs) -> Result<()> {
    match args.command {
        DeviceCommands::List(list_args) => handle_list(list_args),
    }
}

fn handle_list(_args: ListArgs) -> Result<()> {
    let config = Config::load(None)?;
    let devices = list_targets(&config)?;

    if devices.is_empty() {
        println!("No active devices found.");
    } else {
        for (name, target) in devices {
            println!("{} ({})", name, target);
        }
    }

    Ok(())
}
