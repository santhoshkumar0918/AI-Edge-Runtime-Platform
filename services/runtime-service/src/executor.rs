use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::debug;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::broadcast;
use crate::state::BROADCASTS;
use crate::types::ExecutionResult;
use crate::state::JOB_STORE;
use uuid::Uuid;

pub async fn run_python(code: &str, dur: Duration) -> anyhow::Result<(String, String, Option<i32>)> {
    for prog in &["python3", "python"] {
        let mut cmd = Command::new(prog);
        cmd.arg("-c").arg(code);
        match timeout(dur, cmd.output()).await {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let code = output.status.code();
                return Ok((stdout, stderr, code));
            }
            Ok(Err(e)) => {
                debug!(program = %prog, error = %e, "failed to run with program");
                continue;
            }
            Err(_) => {
                return Err(anyhow::anyhow!("execution timed out"));
            }
        }
    }

    Err(anyhow::anyhow!("no python interpreter found"))
}

pub async fn run_python_stream(id: String, code: &str, dur: Duration) -> anyhow::Result<()> {
    // create broadcast channel for this job
    let (tx, _rx) = broadcast::channel(100);
    BROADCASTS.insert(id.clone(), tx.clone());

    for prog in &["python3", "python"] {
        let mut cmd = Command::new(prog);
        cmd.arg("-c").arg(code);
        match cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn() {
            Ok(mut child) => {
                // read stdout
                if let Some(stdout) = child.stdout.take() {
                    let mut reader = BufReader::new(stdout).lines();
                    while let Ok(Some(line)) = reader.next_line().await {
                        let _ = tx.send(format!("OUT: {}", line));
                    }
                }
                // read stderr (not streamed concurrently for simplicity)
                let output = timeout(dur, child.wait_with_output()).await;
                match output {
                    Ok(Ok(output)) => {
                        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                        let code = output.status.code();
                        let result = ExecutionResult {
                            id: id.clone(),
                            status: "completed".into(),
                            stdout: stdout.clone(),
                            stderr: stderr.clone(),
                            exit_code: code,
                        };
                        JOB_STORE.insert(id.clone(), Some(result));
                        let _ = tx.send(format!("DONE: exit={:?}", code));
                        BROADCASTS.remove(&id);
                        return Ok(());
                    }
                    Ok(Err(e)) => {
                        let err = format!("error: {}", e);
                        JOB_STORE.insert(id.clone(), Some(ExecutionResult { id: id.clone(), status: "failed".into(), stdout: "".into(), stderr: err.clone(), exit_code: None }));
                        let _ = tx.send(format!("ERR: {}", err));
                        BROADCASTS.remove(&id);
                        return Ok(());
                    }
                    Err(_) => {
                        let err = "execution timed out".to_string();
                        JOB_STORE.insert(id.clone(), Some(ExecutionResult { id: id.clone(), status: "failed".into(), stdout: "".into(), stderr: err.clone(), exit_code: None }));
                        let _ = tx.send(format!("TIMEOUT"));
                        BROADCASTS.remove(&id);
                        return Ok(());
                    }
                }
            }
            Err(e) => {
                debug!(program = %prog, error = %e, "failed to spawn");
                continue;
            }
        }
    }

    // no interpreter found
    let err = "no python interpreter found".to_string();
    JOB_STORE.insert(id.clone(), Some(ExecutionResult { id: id.clone(), status: "failed".into(), stdout: "".into(), stderr: err.clone(), exit_code: None }));
    let _ = tx.send(format!("ERR: {}", err));
    BROADCASTS.remove(&id);
    Ok(())
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
