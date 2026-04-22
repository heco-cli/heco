use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;

pub struct CommandRunner {
    working_dir: PathBuf,
    env: std::collections::HashMap<String, String>,
}

impl CommandRunner {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            working_dir,
            env: std::collections::HashMap::new(),
        }
    }

    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }

    pub fn run_with_handler<F>(
        &self,
        program: &str,
        args: &[&str],
        mut handler: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(&str),
    {
        let (reader, writer) = os_pipe::pipe()?;

        let mut cmd = Command::new(program);
        let child = cmd
            .args(args)
            .current_dir(&self.working_dir)
            .stdin(Stdio::null())
            .stdout(writer.try_clone()?)
            .stderr(writer);

        for (key, value) in &self.env {
            child.env(key, value);
        }

        let mut child = child.spawn()?;

        // 丢弃 Command 对象以关闭父进程中的管道写入端
        drop(cmd);

        // 读取输出并按行处理，同时处理回车符
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            let mut buf = [0; 1024]; // 使用更大的缓冲区
            match reader.read(&mut buf) {
                Ok(0) => {
                    // 管道关闭，处理剩余内容
                    if !line.is_empty() {
                        handler(&line);
                    }
                    break;
                }
                Ok(n) => {
                    // 尝试将字节解码为UTF-8字符串
                    if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                        // 处理解码后的字符串
                        for c in s.chars() {
                            match c {
                                '\n' => {
                                    // 换行符，处理完整行
                                    if !line.is_empty() {
                                        handler(&line);
                                        line.clear();
                                    }
                                }
                                '\r' => {
                                    // 回车符，处理当前行（通常是进度条更新）
                                    if !line.is_empty() {
                                        handler(&line);
                                        line.clear();
                                    }
                                }
                                _ => {
                                    // 普通字符，添加到当前行
                                    line.push(c);
                                }
                            }
                        }
                    }
                }
                Err(_) => break,
            }
        }

        // 等待子进程完成
        let status = child.wait()?;

        if !status.success() {
            anyhow::bail!("命令执行失败: {} {}", program, args.join(" "));
        }

        Ok(())
    }

    pub fn run_captured_merged(
        &self,
        program: &str,
        args: &[&str],
    ) -> anyhow::Result<std::process::Output> {
        self.run_captured_merged_with_timeout(program, args, None)
    }

    pub fn run_captured_merged_with_timeout(
        &self,
        program: &str,
        args: &[&str],
        timeout: Option<std::time::Duration>,
    ) -> anyhow::Result<std::process::Output> {
        let (reader, writer) = os_pipe::pipe()?;

        let mut cmd = Command::new(program);
        let child = cmd
            .args(args)
            .current_dir(&self.working_dir)
            .stdin(Stdio::null())
            .stdout(writer.try_clone()?)
            .stderr(writer);

        for (key, value) in &self.env {
            child.env(key, value);
        }

        let mut child = child.spawn()?;

        // 丢弃 Command 对象以关闭父进程中的管道写入端，
        // 否则即使子进程退出，读操作也会因为写入端未关闭而一直阻塞。
        drop(cmd);

        // 创建一个通道来接收输出
        let (output_tx, output_rx) = mpsc::channel();

        // 启动一个线程来读取输出
        std::thread::spawn(move || {
            let mut reader = BufReader::new(reader);
            let mut buf = [0u8; 4096];
            let mut output = Vec::new();

            // 持续读取直到管道关闭
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => output.extend_from_slice(&buf[..n]),
                    Err(_) => break,
                }
            }

            // 发送输出到通道
            let _ = output_tx.send(output);
        });

        // 根据是否设置了超时来决定是否等待子进程完成
        let status = if timeout.is_some() {
            // 设置了超时，不等待子进程完成
            // 子进程会继续运行，与主进程分离
            use std::process::ExitStatus;
            #[cfg(unix)]
            let status = ExitStatus::default();
            #[cfg(windows)]
            let status = ExitStatus::default();
            status
        } else {
            // 没有设置超时，等待子进程完成
            child.wait()?
        };

        // 尝试读取输出
        let output = if let Some(timeout) = timeout {
            // 设置了超时，尝试读取输出但不阻塞
            output_rx.recv_timeout(timeout).unwrap_or_default()
        } else {
            // 没有设置超时，等待输出读取完成
            output_rx.recv().unwrap_or_default()
        };

        Ok(std::process::Output {
            status,
            stdout: output,
            stderr: Vec::new(),
        })
    }
}
