use axum::{extract::Json, http::StatusCode, response::IntoResponse, routing::post, Router};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(Deserialize)]
struct ExecutionRequest {
    language: String,
    code: String,
    timeout_ms: Option<u64>,
}

#[derive(Serialize)]
struct ExecutionResult {
    id: String,
    status: String,
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
}

async fn execute_handler(Json(req): Json<ExecutionRequest>) -> impl IntoResponse {
    let id = Uuid::new_v4().to_string();
    info!(id = %id, language = %req.language, "starting execution");

    let timeout_dur = Duration::from_millis(req.timeout_ms.unwrap_or(5000));

    // currently only python supported
    let res = match req.language.as_str() {
        "python" | "py" => run_python(&req.code, timeout_dur).await,
        _ => Err(anyhow::anyhow!("unsupported language")),
    };

    match res {
        Ok((stdout, stderr, exit_code)) => {
            let body = ExecutionResult {
                id,
                status: "completed".into(),
                stdout,
                stderr,
                exit_code,
            };
            (StatusCode::OK, Json(body))
        }
        Err(e) => {
            error!(%e, "execution failed");
            let body = ExecutionResult {
                id,
                status: "failed".into(),
                stdout: "".into(),
                stderr: format!("{}", e),
                exit_code: None,
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(body))
        }
    }
}

async fn run_python(code: &str, dur: Duration) -> anyhow::Result<(String, String, Option<i32>)> {
    // try python3 then python
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
                // failed to spawn this interpreter; try next
                tracing::debug!(program = %prog, error = %e, "failed to run with program");
                continue;
            }
            Err(_) => {
                // timeout
                return Err(anyhow::anyhow!("execution timed out"));
            }
        }
    }

    Err(anyhow::anyhow!("no python interpreter found"))
}

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let app = Router::new().route("/execute", post(execute_handler));

    let addr = SocketAddr::from(([127, 0, 0, 1], 8081));
    info!(%addr, "runtime-service listening");
    hyper::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
