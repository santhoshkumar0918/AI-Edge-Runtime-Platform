use std::{path::Path, path::PathBuf, sync::Arc};

use anyhow::Context;
use uuid::Uuid;
use tokio::fs;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{broadcast, Mutex},
    time::{timeout, Duration, Instant, sleep},
};
use crate::{
    state::{BROADCASTS, JOB_STORE, METRICS, RUNNING_CHILDREN},
    types::ExecutionResult,
};

use serde_json::Value;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use hex;
use reqwest;

const SANDBOX_DIR: &str = "/sandbox";

#[derive(Debug, Clone)]
struct SandboxConfig {
    memory_mb: u64,
    cpus: String,
    pids_limit: u64,
}

impl SandboxConfig {
    fn from_env() -> Self {
        Self {
            memory_mb: std::env::var("EXECUTOR_MEMORY_MB").ok().and_then(|value| value.parse().ok()).unwrap_or(256),
            cpus: std::env::var("EXECUTOR_CPUS").unwrap_or_else(|_| "1.0".to_string()),
            pids_limit: std::env::var("EXECUTOR_PIDS_LIMIT").ok().and_then(|value| value.parse().ok()).unwrap_or(64),
        }
    }
}

fn sandbox_config() -> SandboxConfig {
    SandboxConfig::from_env()
}

#[derive(Debug, Clone)]
struct LangConfig {
    extension: &'static str,
    default_image: &'static str,
    docker_cmd: Vec<String>,
    local_cmds: Vec<Vec<String>>,
}

fn get_lang_config(lang: &str) -> Option<LangConfig> {
    match lang.to_lowercase().as_str() {
        "python" | "py" => Some(LangConfig {
            extension: "py",
            default_image: "python:3.12-slim",
            docker_cmd: vec!["python".into(), "-u".into(), "/sandbox/main.py".into()],
            local_cmds: vec![
                vec!["python3".into()],
                vec!["python".into()],
            ],
        }),
        "javascript" | "js" => Some(LangConfig {
            extension: "js",
            default_image: "node:20-slim",
            docker_cmd: vec!["node".into(), "/sandbox/main.js".into()],
            local_cmds: vec![
                vec!["node".into()],
            ],
        }),
        "bash" | "sh" => Some(LangConfig {
            extension: "sh",
            default_image: "ubuntu:latest",
            docker_cmd: vec!["bash".into(), "/sandbox/main.sh".into()],
            local_cmds: vec![
                vec!["bash".into()],
                vec!["sh".into()],
            ],
        }),
        _ => None,
    }
}

async fn write_code_to_tempfile_async(code: &str, extension: &str) -> anyhow::Result<PathBuf> {
    let dir = std::env::var("EXECUTOR_TMP_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let name = format!("runtime-{}.{}", Uuid::new_v4(), extension);
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

async fn run_local_code(lang_cfg: &LangConfig, script_path: &Path, dur: Duration) -> anyhow::Result<(String, String, Option<i32>)> {
    for cmd_parts in &lang_cfg.local_cmds {
        let binary = &cmd_parts[0];
        let mut command = Command::new(binary);
        for arg in &cmd_parts[1..] {
            command.arg(arg);
        }
        command.arg(script_path);

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

    Err(anyhow::anyhow!("no local interpreter found for extension {}", lang_cfg.extension))
}

pub async fn execute_code(language: &str, code: &str, dur: Duration) -> anyhow::Result<(String, String, Option<i32>)> {
    let lang_cfg = get_lang_config(language)
        .ok_or_else(|| anyhow::anyhow!("unsupported language: {}", language))?;

    match run_container_code(&lang_cfg, code, dur).await {
        Ok(result) => Ok(result),
        Err(err) if cfg!(test) || std::env::var("EXECUTOR_LOCAL_FALLBACK").unwrap_or_default() == "1" => {
            let script_path = write_code_to_tempfile_async(code, lang_cfg.extension).await?;
            let res = run_local_code(&lang_cfg, &script_path, dur).await;
            cleanup_tempfile(&script_path).await;
            res.map_err(|local_err| {
                anyhow::anyhow!("container runner failed: {}; local fallback failed: {}", err, local_err)
            })
        }
        Err(err) => Err(err),
    }
}

pub async fn execute_code_stream(id: String, language: &str, code: &str, dur: Duration) -> anyhow::Result<()> {
    let lang_cfg = get_lang_config(language)
        .ok_or_else(|| anyhow::anyhow!("unsupported language: {}", language))?;

    let started_at = Instant::now();
    // create broadcast channel for logs and register it
    let (tx, _rx) = broadcast::channel(100);
    BROADCASTS.insert(id.clone(), tx.clone());

    let script_path = write_code_to_tempfile_async(code, lang_cfg.extension).await?;
    let config = sandbox_config();

    let run_local = if cfg!(test) || std::env::var("EXECUTOR_LOCAL_FALLBACK").unwrap_or_default() == "1" {
        std::env::var("EXECUTOR_USE_DOCKER").unwrap_or_default() != "1"
    } else {
        false
    };

    let mut command = if run_local {
        let mut local_cmd = None;
        for cmd_parts in &lang_cfg.local_cmds {
            let binary = &cmd_parts[0];
            let mut cmd = Command::new(binary);
            for arg in &cmd_parts[1..] {
                cmd.arg(arg);
            }
            cmd.arg(&script_path);
            local_cmd = Some(cmd);
            break;
        }
        local_cmd.ok_or_else(|| anyhow::anyhow!("no local command configuration available"))?
    } else {
        let mut cmd = Command::new("docker");
        for arg in docker_command_args(&script_path, &config, &lang_cfg) {
            cmd.arg(arg);
        }
        cmd
    };

    command.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());

    // prepare a slot to track the running child so we can cancel it from handlers
    let running_slot = Arc::new(Mutex::new(None));
    RUNNING_CHILDREN.insert(id.clone(), running_slot.clone());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            if !run_local && (cfg!(test) || std::env::var("EXECUTOR_LOCAL_FALLBACK").unwrap_or_default() == "1") {
                let mut fallback_cmd = None;
                for cmd_parts in &lang_cfg.local_cmds {
                    let binary = &cmd_parts[0];
                    let mut cmd = Command::new(binary);
                    for arg in &cmd_parts[1..] {
                        cmd.arg(arg);
                    }
                    cmd.arg(&script_path);
                    fallback_cmd = Some(cmd);
                    break;
                }
                if let Some(mut cmd) = fallback_cmd {
                    cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
                    match cmd.spawn() {
                        Ok(c) => c,
                        Err(local_e) => {
                            let err = format!("failed to start sandbox container: {}; local fallback failed: {}", e, local_e);
                            JOB_STORE.insert(
                                id.clone(),
                                Some(ExecutionResult {
                                    id: id.clone(),
                                    status: "failed".into(),
                                    stdout: "".into(),
                                    stderr: err.clone(),
                                    exit_code: None,
                                    created_at: Some(chrono::Utc::now().timestamp_millis()),
                                }),
                            );
                            let _ = tx.send(format!("ERR: {}", err));
                            BROADCASTS.remove(&id);
                            RUNNING_CHILDREN.remove(&id);
                            let _ = cleanup_tempfile(&script_path).await;
                            return Ok(());
                        }
                    }
                } else {
                    let err = format!("failed to start sandbox container: {}", e);
                    JOB_STORE.insert(
                        id.clone(),
                        Some(ExecutionResult {
                            id: id.clone(),
                            status: "failed".into(),
                            stdout: "".into(),
                            stderr: err.clone(),
                            exit_code: None,
                            created_at: Some(chrono::Utc::now().timestamp_millis()),
                        }),
                    );
                    let _ = tx.send(format!("ERR: {}", err));
                    BROADCASTS.remove(&id);
                    RUNNING_CHILDREN.remove(&id);
                    let _ = cleanup_tempfile(&script_path).await;
                    return Ok(());
                }
            } else {
                let err = format!("failed to start sandbox container: {}", e);
                JOB_STORE.insert(
                    id.clone(),
                    Some(ExecutionResult {
                        id: id.clone(),
                        status: "failed".into(),
                        stdout: "".into(),
                        stderr: err.clone(),
                        exit_code: None,
                        created_at: Some(chrono::Utc::now().timestamp_millis()),
                    }),
                );
                let _ = tx.send(format!("ERR: {}", err));
                BROADCASTS.remove(&id);
                RUNNING_CHILDREN.remove(&id);
                let _ = cleanup_tempfile(&script_path).await;
                return Ok(());
            }
        }
    };

    // take stdout/stderr handles before moving the child into the shared slot
    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    // place the child into the running slot so cancel can access it
    {
        let mut guard = running_slot.lock().await;
        *guard = Some(child);
    }

    let stdout_buffer = Arc::new(Mutex::new(String::new()));
    let stderr_buffer = Arc::new(Mutex::new(String::new()));

    // spawn readers for stdout/stderr if available
    let mut reader_tasks = Vec::new();
    if let Some(out) = stdout_handle {
        let txc = tx.clone();
        let buf = Arc::clone(&stdout_buffer);
        reader_tasks.push(tokio::spawn(async move {
            pump_stream_lines(BufReader::new(out).lines(), "OUT:", txc, buf).await;
        }));
    }
    if let Some(err) = stderr_handle {
        let txc = tx.clone();
        let buf = Arc::clone(&stderr_buffer);
        reader_tasks.push(tokio::spawn(async move {
            pump_stream_lines(BufReader::new(err).lines(), "ERR:", txc, buf).await;
        }));
    }

    // spawn a waiter task that takes ownership of the child and awaits its exit
    let id_clone = id.clone();
    let wait_slot = running_slot.clone();
    tokio::spawn(async move {
        // take the child out of the slot for waiting
        let child_opt = {
            let mut guard = wait_slot.lock().await;
            guard.take()
        };

        if let Some(mut child) = child_opt {
            match timeout(dur, child.wait()).await {
                Ok(Ok(status)) => {
                    // wait for readers to finish
                    for t in reader_tasks {
                        let _ = t.await;
                    }
                    let stdout = stdout_buffer.lock().await.clone();
                    let stderr = stderr_buffer.lock().await.clone();
                    let status_text = if status.success() { "completed" } else { "failed" };
                    let elapsed_ms = started_at.elapsed().as_millis() as u64;
                    if status.success() {
                        METRICS.record_job_completed(elapsed_ms);
                    } else {
                        METRICS.record_job_failed();
                    }
                    finalize_job(&id_clone, &tx, status_text, stdout, stderr, status.code()).await;
                }
                Ok(Err(e)) => {
                    for t in reader_tasks {
                        let _ = t.await;
                    }
                    let err = format!("error: {}", e);
                    METRICS.record_job_failed();
                    finalize_job(&id_clone, &tx, "failed", String::new(), err, None).await;
                }
                Err(_) => {
                    let _ = child.kill().await;
                    for t in reader_tasks {
                        let _ = t.await;
                    }
                    METRICS.record_job_failed();
                    finalize_job(&id_clone, &tx, "failed", String::new(), "execution timed out".to_string(), None).await;
                }
            }
        } else {
            // no child available; finalize as failed
            METRICS.record_job_failed();
            finalize_job(&id_clone, &tx, "failed", String::new(), "no child process".to_string(), None).await;
        }

        // cleanup tempfile
        let _ = cleanup_tempfile(&script_path).await;
    });

    Ok(())
}

fn docker_command_args(script_path: &Path, config: &SandboxConfig, lang_cfg: &LangConfig) -> Vec<String> {
    let sandbox_script_path = format!("{}/main.{}", SANDBOX_DIR, lang_cfg.extension);
    let image = match lang_cfg.extension {
        "py" => std::env::var("EXECUTOR_IMAGE_PYTHON").unwrap_or_else(|_| std::env::var("EXECUTOR_IMAGE").unwrap_or_else(|_| lang_cfg.default_image.to_string())),
        "js" => std::env::var("EXECUTOR_IMAGE_JS").unwrap_or_else(|_| lang_cfg.default_image.to_string()),
        "sh" => std::env::var("EXECUTOR_IMAGE_BASH").unwrap_or_else(|_| lang_cfg.default_image.to_string()),
        _ => lang_cfg.default_image.to_string(),
    };

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
        format!("{}:{}:ro", script_path.display(), sandbox_script_path),
        image,
        lang_cfg.docker_cmd[0].clone(),
        lang_cfg.docker_cmd[1].clone(),
        sandbox_script_path,
    ]
}

async fn run_container_code(lang_cfg: &LangConfig, code: &str, dur: Duration) -> anyhow::Result<(String, String, Option<i32>)> {
    let script_path = write_code_to_tempfile_async(code, lang_cfg.extension).await?;
    let config = sandbox_config();

    let mut command = Command::new("docker");
    for arg in docker_command_args(&script_path, &config, lang_cfg) {
        command.arg(arg);
    }
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
    let now = chrono::Utc::now().timestamp_millis();
    let result = ExecutionResult {
        id: id.to_string(),
        status: status.to_string(),
        stdout: stdout.clone(),
        stderr: stderr.clone(),
        exit_code,
        created_at: Some(now),
    };
    JOB_STORE.insert(id.to_string(), Some(result.clone()));
    crate::state::JOB_META.insert(id.to_string(), now);
    let _ = tx.send(format!("DONE: exit={:?}", exit_code));
    BROADCASTS.remove(id);
    // ensure any running-child slot is removed to avoid leaks
    crate::state::RUNNING_CHILDREN.remove(id);

    // delegate webhook delivery to background worker (non-blocking)
    if std::env::var("JOB_WEBHOOK_URL").is_ok() {
        let id = id.to_string();
        let stdout = stdout.clone();
        let stderr = stderr.clone();
        let status_owned = status.to_string();
        tokio::spawn(async move {
            let _ = send_webhook(&id, &status_owned, exit_code, stdout, stderr, now).await;
        });
    }
}

pub(crate) async fn send_webhook(id: &str, status: &str, exit_code: Option<i32>, stdout: String, stderr: String, now: i64) {
    if let Ok(url) = std::env::var("JOB_WEBHOOK_URL") {
        let payload = serde_json::json!({
            "id": id,
            "status": status,
            "created_at": now,
            "exit_code": exit_code,
            "stdout": if stdout.len() > 1000 { &stdout[..1000] } else { &stdout },
            "stderr": if stderr.len() > 1000 { &stderr[..1000] } else { &stderr },
        });

        let url_clone = url.clone();
        let payload_clone: Value = payload.clone();
        let max_retries: u32 = std::env::var("JOB_WEBHOOK_RETRIES").ok().and_then(|s| s.parse().ok()).unwrap_or(3);
        let secret = std::env::var("JOB_WEBHOOK_SECRET").ok();

        let client = reqwest::Client::builder()
            .user_agent("runtime-service-webhook/1.0")
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let body_str = payload_clone.to_string();

        for attempt in 0..=max_retries {
            let mut req = client.post(&url_clone).header("content-type", "application/json").body(body_str.clone());
            if let Some(ref sec) = secret {
                let mut mac = Hmac::<Sha256>::new_from_slice(sec.as_bytes()).expect("HMAC can take key of any size");
                mac.update(body_str.as_bytes());
                let sig = mac.finalize().into_bytes();
                let sig_hex = hex::encode(sig);
                req = req.header("X-Signature", format!("sha256={}", sig_hex));
            }

            match req.send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        METRICS.record_webhook_success();
                        tracing::info!(url = %url_clone, attempt, "webhook delivered");
                        return;
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_else(|_| "<body read error>".into());
                        tracing::warn!(attempt, url = %url_clone, status = %status, body = %body, "webhook delivery non-success");
                    }
                }
                Err(e) => {
                    tracing::warn!(attempt, url = %url_clone, error = %e, "webhook delivery failed");
                }
            }

            if attempt == max_retries {
                METRICS.record_webhook_failure();
                tracing::error!(url = %url_clone, "webhook delivery failed after retries");
                break;
            }

            // exponential backoff with cap
            let backoff_secs = std::cmp::min(30, 2u64.pow((attempt + 1).min(6)));
            sleep(Duration::from_secs(backoff_secs)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    use axum::{routing::post, Router};
    use std::sync::Arc;
    use tokio::sync::oneshot;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_tempfile_lifecycle() {
        let path = write_code_to_tempfile_async("print('ok')", "py").await.expect("tempfile");
        let content = tokio::fs::read_to_string(&path).await.expect("read tempfile");
        assert_eq!(content, "print('ok')");
        cleanup_tempfile(&path).await;
        assert!(!tokio::fs::try_exists(&path).await.expect("exists check"));
    }

    #[tokio::test]
    async fn test_docker_command_has_expected_flags() {
        let config = SandboxConfig {
            memory_mb: 256,
            cpus: "1.0".into(),
            pids_limit: 64,
        };
        let path = Path::new("/tmp/main.py");
        let lang_cfg = get_lang_config("python").unwrap();
        let args = docker_command_args(path, &config, &lang_cfg);
        assert!(args.contains(&"run".to_string()));
        assert!(args.contains(&"--rm".to_string()));
        assert!(args.contains(&"--network".to_string()));
        assert!(args.contains(&"none".to_string()));
        assert!(args.iter().any(|arg| arg.contains("/tmp/main.py:/sandbox/main.py:ro")));
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

    #[tokio::test]
    async fn test_webhook_signing_and_retry() {
        // spawn a test server that fails first 2 attempts then succeeds
        let counter = Arc::new(AtomicUsize::new(0));
        let c_clone = counter.clone();

        let app = Router::new().route("/", post(move |headers: axum::http::HeaderMap, bytes: axum::body::Bytes| {
            let c = c_clone.clone();
            async move {
                // verify signature if header present
                if let Some(sig_hdr) = headers.get("x-signature") {
                    let sig_str = sig_hdr.to_str().unwrap_or_default();
                    // compute expected
                    let secret = std::env::var("JOB_WEBHOOK_SECRET").unwrap_or_default();
                    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC init");
                    mac.update(&bytes);
                    let expected = hex::encode(mac.finalize().into_bytes());
                    let expected_hdr = format!("sha256={}", expected);
                    if sig_str != expected_hdr {
                        return (axum::http::StatusCode::UNAUTHORIZED, "bad sig");
                    }
                }

                let prev = c.fetch_add(1, Ordering::SeqCst);
                if prev < 2 {
                    // fail first two attempts
                    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "fail")
                } else {
                    (axum::http::StatusCode::OK, "ok")
                }
            }
        }));

        // bind to port 0 to get an ephemeral port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().unwrap();
        
        let (tx, rx) = oneshot::channel();
        let srv_handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).with_graceful_shutdown(async { let _ = rx.await; }).await;
        });

        let url = format!("http://{}:{}/", addr.ip(), addr.port());
        std::env::set_var("JOB_WEBHOOK_URL", url.clone());
        std::env::set_var("JOB_WEBHOOK_SECRET", "testsecret");
        std::env::set_var("JOB_WEBHOOK_RETRIES", "5");

        // call the helper that sends the webhook
        send_webhook("test-id", "completed", Some(0), "out".to_string(), "".to_string(), chrono::Utc::now().timestamp_millis()).await;

        // wait for server to observe at least 3 attempts
        let start = tokio::time::Instant::now();
        loop {
            if counter.load(Ordering::SeqCst) >= 3 {
                break;
            }
            if start.elapsed() > tokio::time::Duration::from_secs(20) {
                panic!("webhook retries did not complete in time");
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }

        // shutdown server
        let _ = tx.send(());
        let _ = srv_handle.await;
    }
}
