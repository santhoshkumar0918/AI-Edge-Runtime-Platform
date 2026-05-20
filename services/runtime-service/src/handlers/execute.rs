use axum::{extract::Json, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use tokio::time::Duration;
use tracing::{error, info};
use uuid::Uuid;

use crate::executor;

#[derive(Deserialize)]
pub struct ExecutionRequest {
    pub language: String,
    pub code: String,
    pub timeout_ms: Option<u64>,
}

#[derive(Serialize)]
pub struct ExecutionResult {
    pub id: String,
    pub status: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

pub async fn execute_handler(Json(req): Json<ExecutionRequest>) -> impl IntoResponse {
    let id = Uuid::new_v4().to_string();
    info!(id = %id, language = %req.language, "starting execution");

    let timeout_dur = Duration::from_millis(req.timeout_ms.unwrap_or(5000));

    let res = match req.language.as_str() {
        "python" | "py" => executor::run_python(&req.code, timeout_dur).await,
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
