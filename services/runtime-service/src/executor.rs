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
    state::{BROADCASTS, JOB_STORE, RUNNING_CHILDREN},
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

fn sandbox_config() -> SandboxConfig {
    SandboxConfig::from_env()
}

async fn write_code_to_tempfile_async(code: &str) -> anyhow::Result<PathBuf> {
    let dir = std::env::var("EXECUTOR_TMP_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let name = format!("runtime-{}.py", Uuid::new_v4());
    let path = PathBuf::from(format!("{}/{}", dir.trim_end_matches('/'), name));
    fs::write(&path, code.as_bytes()).await.context("failed to write sandbox source file")?;
    Ok(path)
}

async fn cleanup_tempfile(path: &Path) {
    let _ = fs::remove_file(path).await;
}

async fn run_command_output(mut command: Command, dur: Duration) -> anyhow::Result<std::process::Output> {
    match timeout(dur, command.output()).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(e)) => Err(anyhow::anyhow!("failed to start execution: {}", e)),
        Err(_) => Err(anyhow::anyhow!("execution timed out")),
    }
}

async fn run_local_python(code: &str, dur: Duration) -> anyhow::Result<(String, String, Option<i32>)> {
    for program in ["python3", "python"] {
        let mut command = Command::new(program);
        command.arg("-u").arg("-c").arg(code);
        match run_command_output(command, dur).await {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                return Ok((stdout, stderr, output.status.code()));
            }
            Err(err) => {
                if err.to_string().contains("timed out") {
                    return Err(err);
                }
            }
        }
    }

    Err(anyhow::anyhow!("no python interpreter found"))
}

fn docker_command(script_path: &Path, config: &SandboxConfig) -> Command {
    let mut command = Command::new("docker");
    for arg in docker_command_args(script_path, config) {
        command.arg(arg);
    }
    command
}

fn docker_command_args(script_path: &Path, config: &SandboxConfig) -> Vec<String> {
    vec![
        "run".into(),
        "--rm".into(),
        "--network".into(),
        "none".into(),
        "--cap-drop".into(),
        "ALL".into(),
        "--security-opt".into(),
        "no-new-privileges".into(),
        "--pids-limit".into(),
        config.pids_limit.to_string(),
        "--memory".into(),
        format!("{}m", config.memory_mb),
        "--cpus".into(),
        config.cpus.clone(),
        "--read-only".into(),
        "--tmpfs".into(),
        format!("{}:rw,noexec,nosuid,size=64m", SANDBOX_DIR),
        "-v".into(),
        format!("{}:{}:ro", script_path.display(), SANDBOX_SCRIPT_PATH),
        config.image.clone(),
        "python".into(),
        "-u".into(),
        SANDBOX_SCRIPT_PATH.into(),
    ]
}

async fn run_container_python(code: &str, dur: Duration) -> anyhow::Result<(String, String, Option<i32>)> {
    let script_path = write_code_to_tempfile_async(code).await?;
    let config = sandbox_config();

        let command = docker_command(&script_path, &config);
    let output = run_command_output(command, dur).await.map_err(|e| {
        anyhow::anyhow!("failed to start sandbox container: {}", e)
    });

    // cleanup tempfile
    cleanup_tempfile(&script_path).await;

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

#[cfg(test)]
fn docker_command_args(script_path: &Path, config: &SandboxConfig) -> Vec<String> {
    vec![
        "run".into(),
        "--rm".into(),
        "--network".into(),
        "none".into(),
        "--cap-drop".into(),
        "ALL".into(),
        "--security-opt".into(),
        "no-new-privileges".into(),
        "--pids-limit".into(),
        config.pids_limit.to_string(),
        "--memory".into(),
        format!("{}m", config.memory_mb),
        "--cpus".into(),
        config.cpus.clone(),
        "--read-only".into(),
        "--tmpfs".into(),
        format!("{}:rw,noexec,nosuid,size=64m", SANDBOX_DIR),
        "-v".into(),
        format!("{}:{}:ro", script_path.display(), SANDBOX_SCRIPT_PATH),
        config.image.clone(),
        "python".into(),
        "-u".into(),
        SANDBOX_SCRIPT_PATH.into(),
    ]
}

pub async fn run_python(code: &str, dur: Duration) -> anyhow::Result<(String, String, Option<i32>)> {
    match run_container_python(code, dur).await {
        Ok(result) => Ok(result),
        Err(err) if cfg!(test) || std::env::var("EXECUTOR_LOCAL_FALLBACK").unwrap_or_default() == "1" => {
            run_local_python(code, dur).await.map_err(|local_err| {
                anyhow::anyhow!("container runner failed: {}; local fallback failed: {}", err, local_err)
            })
        }
        Err(err) => Err(err),
    }
}

pub async fn run_python_stream(id: String, code: &str, dur: Duration) -> anyhow::Result<()> {
    let (tx, _rx) = broadcast::channel(100);
    BROADCASTS.insert(id.clone(), tx.clone());

    let script_path = write_code_to_tempfile_async(code).await?;
    let config = sandbox_config();
    let mut command = docker_command(&script_path, &config);
    command.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());

    // prepare a slot to track the running child so we can cancel it from handlers
    let running_slot = Arc::new(Mutex::new(None));
    RUNNING_CHILDREN.insert(id.clone(), running_slot.clone());

    let mut child = match command.spawn() {
        Ok(child) => {
            // place child into the running slot for cancellation
            let mut guard = running_slot.lock().await;
            *guard = Some(child);
            // take it back for local control
            guard.take().unwrap()
        }
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
            cleanup_tempfile(&script_path).await;
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
            cleanup_tempfile(&script_path).await;
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
            cleanup_tempfile(&script_path).await;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_tempfile_lifecycle() {
        let path = write_code_to_tempfile_async("print('ok')").await.expect("tempfile");
        let content = tokio::fs::read_to_string(&path).await.expect("read tempfile");
        assert_eq!(content, "print('ok')");
        cleanup_tempfile(&path).await;
        assert!(!tokio::fs::try_exists(&path).await.expect("exists check"));
    }

    #[tokio::test]
    async fn test_docker_command_has_expected_flags() {
        let config = SandboxConfig {
            image: "python:3.12-slim".into(),
            memory_mb: 256,
            cpus: "1.0".into(),
            pids_limit: 64,
        };
        let path = Path::new("/tmp/main.py");
        let args = docker_command_args(path, &config);
        assert!(args.contains(&"run".to_string()));
        assert!(args.contains(&"--rm".to_string()));
        assert!(args.contains(&"--network".to_string()));
        assert!(args.contains(&"none".to_string()));
        assert!(args.iter().any(|arg| arg.contains("/tmp/main.py:/sandbox/main.py:ro")));
        assert!(args.contains(&"python:3.12-slim".to_string()));
    }

    #[tokio::test]
    async fn test_pump_stream_lines_collects_output() {
        let (mut writer, reader) = tokio::io::duplex(64);
        let (tx, mut rx) = broadcast::channel(8);
        let buffer = Arc::new(Mutex::new(String::new()));
        let buffer_clone = Arc::clone(&buffer);

        let handle = tokio::spawn(async move {
            pump_stream_lines(BufReader::new(reader).lines(), "OUT:", tx, buffer_clone).await;
        });

        writer.write_all(b"first\nsecond\n").await.expect("write lines");
        writer.shutdown().await.expect("shutdown writer");

        handle.await.expect("task joins");

        let collected = buffer.lock().await.clone();
        assert!(collected.contains("first"));
        assert!(collected.contains("second"));

        let mut messages = Vec::new();
        while let Ok(message) = rx.try_recv() {
            messages.push(message);
        }
        assert!(messages.iter().any(|message| message.contains("OUT: first")));
        assert!(messages.iter().any(|message| message.contains("OUT: second")));
    }
}
