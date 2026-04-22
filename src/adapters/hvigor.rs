use crate::build::BuildArgs;
use crate::clean::CleanArgs;
use crate::command::CommandRunner;
use crate::config::Config;
use crate::project::{ModuleType, load_project};
use anstream::println;
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LogType {
    Warning,
    Error,
}

// 预定义日志前缀映射
static LOG_PREFIX_MAP: LazyLock<HashMap<LogType, Vec<&'static str>>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(
        LogType::Warning,
        // 注意顺序：将最长的前缀放在最前面，防止短前缀（如 "warning:"）提前被匹配
        vec![
            "WARN: WARN: ArkTS:WARN File:",
            "WARN: ArkTS:WARN File:",
            "WARN: ArkTS:WARN",
            "ArkTS:WARN File:",
            "WARN:",
            "ArkTS:WARN",
        ],
    );
    m.insert(LogType::Error, vec!["ERROR: ArkTS:ERROR", "ERROR:"]);
    m
});

/// 识别行是否匹配指定的日志类型，如果匹配则返回 (LogType, 剥离前缀后的内容)
fn parse_log_type(line: &str) -> Option<(LogType, String)> {
    let line_trim = line.trim();
    for (log_type, prefixes) in LOG_PREFIX_MAP.iter() {
        for prefix in prefixes {
            if line_trim.starts_with(prefix) {
                // 如果找到匹配，截取掉前缀并返回
                let content = line_trim.strip_prefix(prefix).unwrap().trim().to_string();
                return Some((*log_type, content));
            }
        }
    }
    None
}

/// 处理 hvigor 日志块的函数
fn process_log_block(block: &[String], width: usize) {
    if block.is_empty() {
        return;
    }

    // 标记上一行是否是 Warning/Error，用于处理多行日志的缩进延续
    let mut last_log_type: Option<LogType> = None;

    // 处理每一行
    for (i, line) in block.iter().enumerate() {
        // 去掉颜色控制字符及其他 ANSI 转义序列
        let mut processed_line = anstream::adapter::strip_str(line).to_string();

        // 忽略空行
        if processed_line.trim().is_empty() {
            continue;
        }

        // 处理第一行（块的头部）
        if i == 0 {
            // 去掉 > hvigor 前缀
            if processed_line.starts_with("> hvigor ") {
                processed_line = processed_line
                    .trim_start_matches("> hvigor")
                    .trim_start()
                    .to_string();
            }
        }

        // 尝试解析当前行是否是 Warning 或 Error
        if let Some((log_type, content)) = parse_log_type(&processed_line) {
            last_log_type = Some(log_type);
            match log_type {
                LogType::Warning => {
                    println!("{}: {}", "warning".yellow().bold(), content);
                }
                LogType::Error => {
                    println!("{}: {}", "error".red().bold(), content);
                }
            }
        } else {
            // 如果当前行不是显式的 Warning/Error 前缀开头

            // 如果这一行是以空白字符开头，且上一行是 Warning/Error，我们认为这是多行日志的延续
            if line.starts_with(char::is_whitespace)
                && let Some(log_type) = last_log_type
            {
                // 延续上一行的颜色风格，但保持其原有的缩进格式
                match log_type {
                    LogType::Warning => println!("{}", processed_line),
                    LogType::Error => println!("{}", processed_line),
                }
                continue;
            }

            // 否则这是一个普通的日志行
            last_log_type = None; // 断开延续状态

            if i == 0 {
                // 第一行，添加绿色的 hvigor: 前缀
                println!(
                    "{:>width$} {}",
                    "hvigor".green().bold(),
                    processed_line,
                    width = width
                );
            } else {
                // 后续行，直接打印原始信息
                println!("{}", processed_line);
            }
        }
    }
}

/// 运行命令并处理日志块
fn run_command_with_log_handling(
    runner: &CommandRunner,
    node_path_str: &str,
    program_args: &[&str],
    width: usize,
) -> anyhow::Result<()> {
    // 维护日志块
    let mut current_block = Vec::new();
    runner.run_with_handler(node_path_str, program_args, |line| {
        // 检查是否是新的日志块开头
        if line.trim().starts_with("> hvigor ") {
            // 处理上一个日志块
            if !current_block.is_empty() {
                process_log_block(&current_block, width);
                current_block.clear();
            }
        }
        // 添加当前行到日志块
        current_block.push(line.to_string());
    })?;
    // 处理最后一个日志块
    if !current_block.is_empty() {
        process_log_block(&current_block, width);
    }
    Ok(())
}

impl BuildArgs {
    pub fn to_command_args(&self, project_root: &PathBuf) -> anyhow::Result<Vec<String>> {
        let mut args = Vec::new();

        if let Some(products) = &self.products {
            args.push("assembleApp".to_string());

            let project = load_project()?;
            if project.root != *project_root {
                anyhow::bail!("project root mismatch");
            }

            // Since build.rs now handles loop logic, self.products should only contain exactly 1 product
            if let Some(p) = products.first() {
                project.validate_product(p)?;
                args.push("-p".to_string());
                args.push(format!("product={}", p));
            }
        } else {
            let (module_name, target_name) = self.parse_module().unwrap_or((String::new(), None));

            let mut tasks = resolve_tasks(&module_name, &target_name, project_root)?;
            args.append(&mut tasks);

            if let Some(module) = &self.module {
                args.push("-p".to_string());
                args.push(format!("module={}", module));
            }
        }

        let mode = if self.release { "release" } else { "debug" };
        args.push("-p".to_string());
        args.push(format!("buildMode={}", mode));

        // 默认禁用 daemon 以避免 uv_cwd 报错问题
        args.push("--no-daemon".to_string());

        Ok(args)
    }
}

pub fn sync(project_root: &Path, config: &Config, quiet: bool, width: usize) -> anyhow::Result<()> {
    let node_path = config
        .node_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 Node 路径"))?;

    let hvigorw_js_path = config
        .hvigorw_js_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 hvigorw.js 路径"))?;

    let sdk_path = config
        .sdk_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 SDK 路径"))?;

    let java_path = config.java_path().ok_or_else(|| {
        anyhow::anyhow!("未找到 Java 路径，请确保 JAVA_HOME 环境变量已设置或 Java 在 PATH 中")
    })?;

    let java_home = java_path.parent().unwrap().parent().unwrap();
    let java_bin = java_home.join("bin");

    let current_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", java_bin.to_str().unwrap_or(""), current_path);

    let runner = CommandRunner::new(project_root.to_path_buf())
        .env("DEVECO_SDK_HOME", sdk_path.to_str().unwrap_or(""))
        .env("JAVA_HOME", java_home.to_str().unwrap_or(""))
        .env("PATH", &new_path);

    let project = load_project()?;
    let product_name = project
        .products
        .first()
        .map(|s| s.as_str())
        .unwrap_or("default");
    let product_arg = format!("product={}", product_name);

    let command_args = [
        "--sync",
        "-p",
        &product_arg,
        "--analyze=normal",
        "--parallel",
        "--incremental",
        "--no-daemon",
    ];

    let program_args: Vec<&str> = std::iter::once(hvigorw_js_path.to_str().unwrap())
        .chain(command_args.iter().copied())
        .collect();

    let node_path_str = node_path.to_str().unwrap_or("node");

    if quiet {
        let output = runner.run_captured_merged(node_path_str, &program_args)?;
        if !output.status.success() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
            anyhow::bail!("{}", "error: hvigor sync failed".red());
        }
        Ok(())
    } else {
        run_command_with_log_handling(&runner, node_path_str, &program_args, width)
    }
}

pub fn build(
    args: &BuildArgs,
    project_root: &PathBuf,
    config: &Config,
    width: usize,
) -> anyhow::Result<()> {
    let node_path = config
        .node_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 Node 路径"))?;

    let hvigorw_js_path = config
        .hvigorw_js_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 hvigorw.js 路径"))?;

    let sdk_path = config
        .sdk_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 SDK 路径"))?;

    let java_path = config.java_path().ok_or_else(|| {
        anyhow::anyhow!("未找到 Java 路径，请确保 JAVA_HOME 环境变量已设置或 Java 在 PATH 中")
    })?;

    let java_home = java_path.parent().unwrap().parent().unwrap();
    let java_bin = java_home.join("bin");

    let current_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", java_bin.to_str().unwrap_or(""), current_path);

    let command_args = args.to_command_args(project_root)?;
    let runner = CommandRunner::new(project_root.clone())
        .env("DEVECO_SDK_HOME", sdk_path.to_str().unwrap_or(""))
        .env("JAVA_HOME", java_home.to_str().unwrap_or(""))
        .env("PATH", &new_path);

    let program_args: Vec<&str> = std::iter::once(hvigorw_js_path.to_str().unwrap())
        .chain(command_args.iter().map(|s| s.as_str()))
        .collect();

    let node_path_str = node_path.to_str().unwrap_or("node");

    if args.quiet {
        let output = runner.run_captured_merged(node_path_str, &program_args)?;
        if !output.status.success() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
            anyhow::bail!("{}", "error: build failed".red());
        }
        Ok(())
    } else {
        run_command_with_log_handling(&runner, node_path_str, &program_args, width)
    }
}

fn resolve_tasks(
    module_name: &str,
    target_name: &Option<String>,
    project_root: &PathBuf,
) -> anyhow::Result<Vec<String>> {
    let project = load_project()?;

    if project.root != *project_root {
        anyhow::bail!("project root mismatch");
    }

    if !module_name.is_empty() {
        if let Some(m) = project.find_module(module_name) {
            if let Some(target) = target_name {
                project.validate_target(module_name, target)?;
            }
            let task = match m.module_type {
                ModuleType::Har => "assembleHar".to_string(),
                ModuleType::Shared => "assembleHsp".to_string(),
                _ => "assembleHap".to_string(),
            };
            return Ok(vec![task]);
        } else {
            let available: Vec<&str> = project.modules.iter().map(|m| m.name.as_str()).collect();
            let msg = format!(
                "error: module '{}' not found in project\n\nAvailable modules:\n  {}",
                module_name.red(),
                available.join("\n  ")
            );
            anyhow::bail!("{}", msg);
        }
    }

    if !project.modules.is_empty() {
        let mut has_hap = false;
        let mut has_hsp = false;
        let mut has_har = false;

        for m in &project.modules {
            match m.module_type {
                ModuleType::Entry | ModuleType::Feature => has_hap = true,
                ModuleType::Shared => has_hsp = true,
                ModuleType::Har => has_har = true,
                _ => has_hap = true,
            }
        }

        let mut tasks = Vec::new();
        if has_hap {
            tasks.push("assembleHap".to_string());
        }
        if has_hsp {
            tasks.push("assembleHsp".to_string());
        }
        if has_har {
            tasks.push("assembleHar".to_string());
        }

        if !tasks.is_empty() {
            return Ok(tasks);
        }
    }

    Ok(vec!["assembleHap".to_string()])
}

pub fn clean(
    args: &CleanArgs,
    project_root: &Path,
    config: &Config,
    width: usize,
) -> anyhow::Result<()> {
    let node_path = config
        .node_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 Node 路径"))?;

    let hvigorw_js_path = config
        .hvigorw_js_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 hvigorw.js 路径"))?;

    let sdk_path = config
        .sdk_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 SDK 路径"))?;

    let mut command_args = vec!["clean".to_string(), "--no-daemon".to_string()];
    if let Some(module) = &args.module {
        command_args.push("-p".to_string());
        command_args.push(format!("module={}", module));
    }

    let runner = CommandRunner::new(project_root.to_path_buf())
        .env("DEVECO_SDK_HOME", sdk_path.to_str().unwrap_or(""));

    let program_args: Vec<&str> = std::iter::once(hvigorw_js_path.to_str().unwrap())
        .chain(command_args.iter().map(|s| s.as_str()))
        .collect();

    let node_path_str = node_path.to_str().unwrap_or("node");

    if args.quiet {
        let output = runner.run_captured_merged(node_path_str, &program_args)?;
        if !output.status.success() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
            anyhow::bail!("{}", "error: clean failed".red());
        }
        Ok(())
    } else {
        run_command_with_log_handling(&runner, node_path_str, &program_args, width)
    }
}
