use crate::adapters::hvigor;
use crate::config::Config;
use crate::project::find_project_root;
use anstream::{eprintln, println};
use clap::Parser;
use clap_complete::engine::ArgValueCompleter;
use owo_colors::OwoColorize;
use std::time::Instant;

#[derive(Parser, Debug)]
pub struct BuildArgs {
    /// Module name (format: module or module@target)
    #[arg(short, long, add = ArgValueCompleter::new(crate::completion::complete_modules))]
    pub module: Option<String>,
    /// Debug build mode
    #[arg(long, conflicts_with = "release")]
    pub debug: bool,
    /// Release build mode
    #[arg(long, conflicts_with = "debug")]
    pub release: bool,
    /// Quiet mode, reduce output
    #[arg(long, short)]
    pub quiet: bool,
    /// Specify one or more product names to build app, separated by commas. If passed without values, builds all products.
    #[arg(long, num_args = 0.., value_delimiter = ',', add = ArgValueCompleter::new(crate::completion::complete_products))]
    pub products: Option<Vec<String>>,
}

impl BuildArgs {
    pub fn parse_module(&self) -> Option<(String, Option<String>)> {
        self.module.as_ref().map(|m| {
            if let Some(idx) = m.find('@') {
                let module_name = m[..idx].to_string();
                let target_name = m[idx + 1..].to_string();
                (module_name, Some(target_name))
            } else {
                (m.clone(), None)
            }
        })
    }
}

pub(crate) fn handle_build(args: BuildArgs) {
    let project_root = match find_project_root() {
        Some(path) => path,
        None => {
            eprintln!(
                "{}",
                "error: no project root found (build-profile.json5)".red()
            );
            std::process::exit(1);
        }
    };

    let config = match Config::load(Some(&project_root)) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("{}", format!("error: failed to load config: {}", e).red());
            std::process::exit(1);
        }
    };

    if !args.quiet {
        println!("{:>9} project", "Syncing".green().bold());
    }
    if let Err(e) = hvigor::sync(&project_root, &config, args.quiet, 9) {
        eprintln!("{}", format!("error: sync failed: {}", e).red());
        std::process::exit(1);
    }

    if let Err(e) = crate::adapters::ohpm::install(&project_root, &config, args.quiet) {
        eprintln!("{}", format!("error: install failed: {}", e).red());
        std::process::exit(1);
    }

    let start = Instant::now();
    let build_type = if args.release { "release" } else { "debug" };

    let project = match crate::project::load_project() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", format!("error: failed to load project: {}", e).red());
            std::process::exit(1);
        }
    };

    let args = if args.module.is_none() && args.products.is_none() {
        if let Ok(current_dir) = std::env::current_dir() {
            if let Some(module) = project.find_module_by_path(&current_dir) {
                BuildArgs {
                    module: Some(module.name.clone()),
                    debug: args.debug,
                    release: args.release,
                    quiet: args.quiet,
                    products: args.products.clone(),
                }
            } else {
                args
            }
        } else {
            args
        }
    } else {
        args
    };

    let (module_name, target_name) = args.parse_module().unwrap_or((String::new(), None));

    if let Some(products) = &args.products {
        let target_products = if products.is_empty() {
            project.products.clone()
        } else {
            products.clone()
        };

        if target_products.is_empty() {
            eprintln!("{}", "error: no products found to build".red());
            std::process::exit(1);
        }

        let total_start = Instant::now();
        for product in &target_products {
            if !args.quiet {
                println!(
                    "{:>9} product {} ({})",
                    "Compiling".green().bold(),
                    product,
                    project_root.display()
                );
            }

            // Create a temporary args just for this product
            let single_product_args = BuildArgs {
                module: args.module.clone(),
                debug: args.debug,
                release: args.release,
                quiet: args.quiet,
                products: Some(vec![product.clone()]),
            };

            match hvigor::build(&single_product_args, &project_root, &config, 9) {
                Ok(_) => {
                    if !args.quiet {
                        println!(
                            "{:>9} {} product {} in {:.2?}",
                            "Finished".green().bold(),
                            build_type,
                            product,
                            start.elapsed()
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "{}",
                        format!("error: build failed for product {}: {}", product, e).red()
                    );
                    std::process::exit(1);
                }
            }
        }

        if target_products.len() > 1 && !args.quiet {
            println!(
                "{:>9} {} product(s) in {:.2?}",
                "Finished".green().bold(),
                build_type,
                total_start.elapsed()
            );
        }
    } else {
        let display_name = if module_name.is_empty() {
            "project".to_string()
        } else if let Some(target) = target_name {
            format!("{}@{}", module_name, target)
        } else {
            module_name.clone()
        };

        if !args.quiet {
            println!(
                "{:>9} {} ({})",
                "Compiling".green().bold(),
                display_name,
                project_root.display()
            );
        }

        match hvigor::build(&args, &project_root, &config, 9) {
            Ok(_) => {
                if !args.quiet {
                    println!(
                        "{:>9} {} module(s) in {:.2?}",
                        "Finished".green().bold(),
                        build_type,
                        start.elapsed()
                    );
                }
            }
            Err(e) => {
                eprintln!("{}", format!("error: build failed: {}", e).red());
                std::process::exit(1);
            }
        }
    }
}
