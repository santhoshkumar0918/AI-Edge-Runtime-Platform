use std::{path::Path, path::PathBuf, sync::Arc};

use anyhow::Context;
use uuid::Uuid;
use tokio::fs;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{broadcast, Mutex},
    time::{timeout, Duration},
};
use crate::{
    state::{BROADCASTS, JOB_STORE},
    types::ExecutionResult,
};

const DEFAULT_CONTAINER_IMAGE: &str = "python:3.12-slim";
const SANDBOX_DIR: &str = "/sandbox";
const SANDBOX_SCRIPT_PATH: &str = "/sandbox/main.py";

#[derive(Debug, Clone)]
struct SandboxConfig {
    image: String,
    memory_mb: u64,
    cpus: String,
    pids_limit: u64,
}

impl SandboxConfig {
    fn from_env() -> Self {
        Self {
            image: std::env::var("EXECUTOR_IMAGE").unwrap_or_else(|_| DEFAULT_CONTAINER_IMAGE.to_string()),
            memory_mb: std::env::var("EXECUTOR_MEMORY_MB")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(256),
            cpus: std::env::var("EXECUTOR_CPUS").unwrap_or_else(|_| "1.0".to_string()),
            pids_limit: std::env::var("EXECUTOR_PIDS_LIMIT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(64),
        }
    }
}

async fn write_code_to_tempfile_async(code: &str) -> anyhow::Result<PathBuf> {
    let dir = std::env::var("EXECUTOR_TMP_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let name = format!("runtime-{}.py", Uuid::new_v4());
    let path = PathBuf::from(format!("{}/{}", dir.trim_end_matches('/'), name));
    fs::write(&path, code.as_bytes()).await.context("failed to write sandbox source file")?;
    Ok(path)
}

fn docker_command(script_path: &Path, config: &SandboxConfig) -> Command {
    let mut command = Command::new("docker");
    command
        .arg("run")
        .arg("--rm")
        .arg("--network")
        .arg("none")
        .arg("--cap-drop")
        .arg("ALL")
        .arg("--security-opt")
        .arg("no-new-privileges")
        .arg("--pids-limit")
        .arg(config.pids_limit.to_string())
        .arg("--memory")
        .arg(format!("{}m", config.memory_mb))
        .arg("--cpus")
        .arg(&config.cpus)
        .arg("--read-only")
        .arg("--tmpfs")
        .arg(format!("{}:rw,noexec,nosuid,size=64m", SANDBOX_DIR))
        .arg("-v")
        .arg(format!("{}:{}:ro", script_path.display(), SANDBOX_SCRIPT_PATH))
        .arg(&config.image)
        .arg("python")
        .arg("-u")
        .arg(SANDBOX_SCRIPT_PATH);
    command
}

async fn run_container_python(code: &str, dur: Duration) -> anyhow::Result<(String, String, Option<i32>)> {
    let script_path = write_code_to_tempfile_async(code).await?;
    let config = SandboxConfig::from_env();

    let mut command = docker_command(&script_path, &config);
    let output = match timeout(dur, command.output()).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(e)) => Err(anyhow::anyhow!("failed to start sandbox container: {}", e)),
        Err(_) => Err(anyhow::anyhow!("execution timed out")),
    };

    // cleanup tempfile
    let _ = fs::remove_file(&script_path).await;

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Ok((stdout, stderr, output.status.code()))
        }
        Err(e) => Err(e),
    }
}

async fn pump_stream_lines<R>(
    mut reader: tokio::io::Lines<BufReader<R>>,
    prefix: &'static str,
    tx: broadcast::Sender<String>,
    buffer: Arc<Mutex<String>>,
)
where
    R: tokio::io::AsyncRead + Unpin,
{
    while let Ok(Some(line)) = reader.next_line().await {
        {
            let mut acc = buffer.lock().await;
            acc.push_str(&line);
            acc.push('\n');
        }
        let _ = tx.send(format!("{} {}", prefix, line));
    }
}

async fn finalize_job(id: &str, tx: &broadcast::Sender<String>, status: &str, stdout: String, stderr: String, exit_code: Option<i32>) {
    let result = ExecutionResult {
        id: id.to_string(),
        status: status.to_string(),
        stdout,
        stderr,
        exit_code,
    };
    JOB_STORE.insert(id.to_string(), Some(result));
    let _ = tx.send(format!("DONE: exit={:?}", exit_code));
    BROADCASTS.remove(id);
}

pub async fn run_python(code: &str, dur: Duration) -> anyhow::Result<(String, String, Option<i32>)> {
    run_container_python(code, dur).await
}

pub async fn run_python_stream(id: String, code: &str, dur: Duration) -> anyhow::Result<()> {
    let (tx, _rx) = broadcast::channel(100);
    BROADCASTS.insert(id.clone(), tx.clone());

    let script_path = write_code_to_tempfile_async(code).await?;
    let config = SandboxConfig::from_env();
    let mut command = docker_command(&script_path, &config);
    command.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            let err = format!("failed to start sandbox container: {}", e);
            JOB_STORE.insert(
                id.clone(),
                Some(ExecutionResult {
                    id: id.clone(),
                    status: "failed".into(),
                    stdout: "".into(),
                    stderr: err.clone(),
                    exit_code: None,
                }),
            );
            let _ = tx.send(format!("ERR: {}", err));
            BROADCASTS.remove(&id);
            return Ok(());
        }
    };

    let stdout_buffer = Arc::new(Mutex::new(String::new()));
    let stderr_buffer = Arc::new(Mutex::new(String::new()));

    let stdout_task = child.stdout.take().map(|stdout| {
        let tx = tx.clone();
        let buffer = Arc::clone(&stdout_buffer);
        tokio::spawn(async move {
            pump_stream_lines(BufReader::new(stdout).lines(), "OUT:", tx, buffer).await;
        })
    });

    let stderr_task = child.stderr.take().map(|stderr| {
        let tx = tx.clone();
        let buffer = Arc::clone(&stderr_buffer);
        tokio::spawn(async move {
            pump_stream_lines(BufReader::new(stderr).lines(), "ERR:", tx, buffer).await;
        })
    });

    match timeout(dur, child.wait()).await {
        Ok(Ok(status)) => {
            if let Some(task) = stdout_task {
                let _ = task.await;
            }
            if let Some(task) = stderr_task {
                let _ = task.await;
            }

            let stdout = stdout_buffer.lock().await.clone();
            let stderr = stderr_buffer.lock().await.clone();
            finalize_job(&id, &tx, if status.success() { "completed" } else { "failed" }, stdout, stderr, status.code()).await;
            // cleanup tempfile
            let _ = fs::remove_file(&script_path).await;
            Ok(())
        }
        Ok(Err(e)) => {
            if let Some(task) = stdout_task {
                let _ = task.await;
            }
            if let Some(task) = stderr_task {
                let _ = task.await;
            }
            let err = format!("error: {}", e);
            finalize_job(&id, &tx, "failed", String::new(), err, None).await;
            let _ = fs::remove_file(&script_path).await;
            Ok(())
        }
        Err(_) => {
            let _ = child.kill().await;
            if let Some(task) = stdout_task {
                let _ = task.await;
            }
            if let Some(task) = stderr_task {
                let _ = task.await;
            }
            finalize_job(&id, &tx, "failed", String::new(), "execution timed out".to_string(), None).await;
            let _ = fs::remove_file(&script_path).await;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_run_python_success() {
        let code = r#"print('hello-from-test')"#;
        let res = run_python(code, Duration::from_secs(2)).await.expect("should run");
        assert!(res.0.contains("hello-from-test"));
        assert_eq!(res.2, Some(0));
    }

    #[tokio::test]
    async fn test_run_python_timeout() {
        let code = r#"import time; time.sleep(1); print('done')"#;
        let res = run_python(code, Duration::from_millis(10)).await;
        assert!(res.is_err());
        let err = format!("{}", res.unwrap_err());
        assert!(err.contains("timed out"));
    }
}
