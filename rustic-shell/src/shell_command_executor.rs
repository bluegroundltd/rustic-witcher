use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{error, info};

pub struct ShellCommandExecutor;

impl ShellCommandExecutor {
    pub async fn execute_cmd(
        cmd_for_execution: impl AsRef<str>,
        check_for_error: Option<bool>,
    ) -> Result<(), String> {
        let check_for_error = check_for_error.unwrap_or(false);
        let mut restore_cmd = tokio::process::Command::new("sh");

        restore_cmd.arg("-c");
        restore_cmd.arg(cmd_for_execution.as_ref());
        restore_cmd.stdout(Stdio::piped());
        restore_cmd.stderr(Stdio::piped());

        let mut child = restore_cmd
            .spawn()
            .expect("failed to spawn command for pg_restore");

        let stdout = child.stdout.take().unwrap_or_else(|| {
            panic!(
                "child did not have a handle to stdout for {}",
                cmd_for_execution.as_ref()
            )
        });

        let stderr = child.stderr.take().unwrap_or_else(|| {
            panic!(
                "child did not have a handle to stderr for {}",
                cmd_for_execution.as_ref()
            )
        });

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut stderr_lines: Vec<String> = Vec::new();
        let mut stderr_error_lines: Vec<String> = Vec::new();

        loop {
            tokio::select! {
                line = stdout_reader.next_line() => {
                    match line.unwrap() {
                        Some(line) => {
                            if check_for_error && line.to_lowercase().contains("error") {
                                error!("{line}");
                                return Err(line);
                            } else {
                                info!("{line}");
                            }
                        }
                        None => break,
                    }
                }
                line = stderr_reader.next_line() => {
                    if let Some(line) = line.unwrap() {
                        let lower = line.to_lowercase();
                        if lower.contains("error") || lower.contains("warning") {
                            stderr_error_lines.push(line.clone());
                        }
                        info!("{line}");
                        stderr_lines.push(line);
                    }
                }
            }
        }

        // Drain remaining stderr after stdout closes
        while let Some(line) = stderr_reader.next_line().await.unwrap() {
            let lower = line.to_lowercase();
            if lower.contains("error") || lower.contains("warning") {
                stderr_error_lines.push(line.clone());
            }
            info!("{line}");
            stderr_lines.push(line);
        }

        let status = child
            .wait()
            .await
            .expect("child process encountered an error");

        if !status.success() {
            for line in &stderr_error_lines {
                error!("{line}");
            }
            let msg = if stderr_error_lines.is_empty() {
                if stderr_lines.is_empty() {
                    format!("command exited with status {status}")
                } else {
                    stderr_lines.join("\n")
                }
            } else {
                stderr_error_lines.join("\n")
            };
            return Err(msg);
        }

        Ok(())
    }
}
