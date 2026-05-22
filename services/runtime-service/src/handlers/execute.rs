use axum::{extract::Json, extract::Path, http::StatusCode, response::IntoResponse};
use tokio::time::Duration;
use tracing::{error, info};
use uuid::Uuid;

use crate::executor;
use crate::state::JOB_STORE;
use crate::types::{ExecutionRequest, ExecutionResult};

pub async fn execute_handler(Json(req): Json<ExecutionRequest>) -> impl IntoResponse {
    // synchronous/blocking execution (keeps previous behavior)
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

pub async fn execute_async_handler(Json(req): Json<ExecutionRequest>) -> impl IntoResponse {
    let id = Uuid::new_v4().to_string();
    info!(id = %id, language = %req.language, "scheduling async execution");

    // mark as running
    JOB_STORE.insert(id.clone(), None);

    let code = req.code.clone();
    let lang = req.language.clone();
    let timeout_dur = Duration::from_millis(req.timeout_ms.unwrap_or(5000));
    let id_clone = id.clone();

    tokio::spawn(async move {
        let res = match lang.as_str() {
            "python" | "py" => executor::run_python(&code, timeout_dur).await,
            _ => Err(anyhow::anyhow!("unsupported language")),
        };

        match res {
            Ok((stdout, stderr, exit_code)) => {
                let body = ExecutionResult {
                    id: id_clone.clone(),
                    status: "completed".into(),
                    stdout,
                    stderr,
                    exit_code,
                };
                JOB_STORE.insert(id_clone, Some(body));
            }
            Err(e) => {
                let body = ExecutionResult {
                    id: id_clone.clone(),
                    status: "failed".into(),
                    stdout: "".into(),
                    stderr: format!("{}", e),
                    exit_code: None,
                };
                JOB_STORE.insert(id_clone, Some(body));
            }
        }
    });

    let resp = serde_json::json!({
        "id": id,
        "status": "scheduled",
        "status_url": "/status/{id}"
    });

    (StatusCode::ACCEPTED, Json(resp))
}

pub async fn status_handler(Path(id): Path<String>) -> impl IntoResponse {
    if let Some(entry) = JOB_STORE.get(&id) {
        match entry.value() {
            None => (StatusCode::ACCEPTED, Json(serde_json::json!({"id": id, "status": "running"}))),
            Some(res) => (StatusCode::OK, Json(serde_json::to_value(res.clone()).unwrap())),
        }
    } else {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not found"})))
    }
}
