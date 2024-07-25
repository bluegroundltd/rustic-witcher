use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::info;

pub struct ShellCommandExecutor;

impl ShellCommandExecutor {
    pub async fn execute_cmd(cmd_for_execution: impl AsRef<str>) {
        let mut restore_cmd = tokio::process::Command::new("sh");

        restore_cmd.arg("-c");
        restore_cmd.arg(cmd_for_execution.as_ref());
        restore_cmd.stdout(Stdio::piped());

        let mut child = restore_cmd
            .spawn()
            .expect("failed to spawn command for pg_restore");

        let stdout = child.stdout.take().unwrap_or_else(|| {
            panic!(
                "child did not have a handle to stdout for {}",
                cmd_for_execution.as_ref()
            )
        });

        let mut reader = BufReader::new(stdout).lines();

        tokio::spawn(async move {
            _ = child
                .wait()
                .await
                .expect("child process encountered an error");
        });

        while let Some(line) = reader.next_line().await.unwrap() {
            info!("{line}");
        }
    }
}
