use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::debug;

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
