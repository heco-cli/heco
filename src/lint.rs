use crate::command::CommandRunner;
use crate::config::Config;
use crate::project::{find_project_root, load_project};
use anstream::println;
use anyhow::{Context, Result};
use clap::Parser;
use owo_colors::OwoColorize;
use std::io::Write;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "lint")]
pub struct LintArgs {
    /// Automatically fix fixable issues
    #[arg(long)]
    pub fix: bool,
    /// Specify one or more product names, separated by commas
    #[arg(long, value_delimiter = ',')]
    pub products: Option<Vec<String>>,
    /// Quiet mode, reduce output
    #[arg(short, long)]
    pub quiet: bool,
}

fn run_codelinter(
    project_root: &std::path::Path,
    config: &Config,
    check_path: &str,
    fix: bool,
    product: Option<&str>,
    quiet: bool,
) -> Result<()> {
    let node_path = config.node_path().context(
        "Node runtime not found. Please check DevEco Studio installation path configuration.",
    )?;

    let codelinter_path = config.codelinter_path().context(
        "codelinter tool not found. Please check DevEco Studio installation path configuration.",
    )?;

    if !codelinter_path.exists() {
        return Err(anyhow::anyhow!(
            "codelinter tool not found at path: {}. Please check DevEco Studio installation path configuration.",
            codelinter_path.display()
        ));
    }

    let codelinter_str = codelinter_path.to_string_lossy().to_string();

    let mut cmd_args: Vec<String> = vec![codelinter_str];

    if fix {
        cmd_args.push("--fix".to_string());
    }

    if let Some(product_name) = product {
        cmd_args.push("-p".to_string());
        cmd_args.push(product_name.to_string());
    }

    if !check_path.is_empty() && check_path != "." {
        cmd_args.push(check_path.to_string());
    }

    let cmd_args_ref: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();
    let node_path_str = node_path.to_str().unwrap_or("node");
    let runner = CommandRunner::new(project_root.to_path_buf());

    if quiet {
        let output = runner.run_captured_merged(node_path_str, &cmd_args_ref)?;
        if !output.status.success() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
            anyhow::bail!("error: lint failed");
        }
        Ok(())
    } else {
        // 捕获并处理输出，特别是进度条
        runner.run_with_handler(node_path_str, &cmd_args_ref, |line| {
            // 清理 ANSI 转义序列和特殊字符（参考 hvigor.rs 的实现）
            let mut cleaned_line = line.to_string();
            // 处理常见的颜色控制序列
            cleaned_line = cleaned_line.replace("\x1b[0m", "");
            cleaned_line = cleaned_line.replace("\x1b[30m", "");
            cleaned_line = cleaned_line.replace("\x1b[31m", "");
            cleaned_line = cleaned_line.replace("\x1b[32m", "");
            cleaned_line = cleaned_line.replace("\x1b[33m", "");
            cleaned_line = cleaned_line.replace("\x1b[34m", "");
            cleaned_line = cleaned_line.replace("\x1b[35m", "");
            cleaned_line = cleaned_line.replace("\x1b[36m", "");
            cleaned_line = cleaned_line.replace("\x1b[37m", "");
            cleaned_line = cleaned_line.replace("\x1b[1m", "");
            cleaned_line = cleaned_line.replace("\x1b[4m", "");
            // 移除回车符和换行符
            cleaned_line = cleaned_line.replace(['\r', '\n'], "");

            // 检查是否是进度条行（包含 Working...[ 或 Finished...[）
            if cleaned_line.contains("Working...[") || cleaned_line.contains("Finished...[") {
                // 提取所有数字，过滤掉颜色代码相关的数字（30-37 是颜色代码，0 是重置）
                let numbers: Vec<u32> = cleaned_line
                    .split(|c: char| !c.is_ascii_digit())
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse().ok())
                    .filter(|&n| n <= 100 && n != 0 && !(30..=37).contains(&n))
                    .collect();

                // 取最后一个数字作为百分比
                if let Some(percent) = numbers.last().copied() {
                    // 清除当前行并显示新的进度条
                    print!("\r");
                    // 使用足够的空格清除之前的内容
                    print!("{:width$}", "", width = 100);
                    print!("\r");
                    let bar_width = 50; // 使用更宽的进度条
                    // 安全计算，确保不会溢出
                    let filled = (bar_width as u32 * percent) / 100;
                    let bar = "=".repeat(filled as usize)
                        + &" ".repeat((bar_width as u32 - filled) as usize);
                    // 确保输出完整的进度条
                    let progress_str =
                        format!("{:>9} [{}] {}/100 ", "Linting".green().bold(), bar, percent);
                    print!("{}", progress_str);
                    std::io::stdout().flush().unwrap();
                    // 如果是100%，换行
                    if percent == 100 {
                        println!();
                    }
                }
            } else if !line.trim().is_empty() {
                // 其他非空白行直接打印，保持右对齐格式
                // 先确保进度条行已经结束
                print!("\r");
                print!("{:width$}", "", width = 100);
                print!("\r");
                println!("{:>9} {}", "Linter".green().bold(), line);
            }
        })
    }
}

pub fn handle_lint(args: LintArgs) -> Result<()> {
    let project_root = find_project_root()
        .context("no HMOS project root found (missing build-profile.json5 or oh-package.json5)")?;

    let project = load_project().context("failed to load project info")?;

    let config = Config::load(Some(&project_root))?;

    let current_dir = std::env::current_dir()?;

    let relative_path = current_dir
        .strip_prefix(&project_root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());

    let check_path = if relative_path.is_empty() || relative_path == "." {
        ".".to_string()
    } else {
        relative_path
    };

    let start = Instant::now();

    if let Some(ref products) = args.products {
        if !args.quiet {
            println!(
                "{:>9} {} ({})",
                "Linting".green().bold(),
                products.join(", "),
                project_root.display()
            );
        }

        for product in products {
            project.validate_product(product)?;
            run_codelinter(
                &project_root,
                &config,
                &check_path,
                args.fix,
                Some(product),
                args.quiet,
            )?;
        }

        if !args.quiet {
            println!(
                "\n{:>9} in {:.2?}",
                "Finished".green().bold(),
                start.elapsed()
            );
        }
    } else {
        if !args.quiet {
            println!(
                "{:>9} ({})",
                "Linting".green().bold(),
                project_root.display()
            );
        }

        run_codelinter(
            &project_root,
            &config,
            &check_path,
            args.fix,
            None,
            args.quiet,
        )?;

        if !args.quiet {
            println!(
                "{:>9} in {:.2?}",
                "Finished".green().bold(),
                start.elapsed()
            );
        }
    }

    Ok(())
}
