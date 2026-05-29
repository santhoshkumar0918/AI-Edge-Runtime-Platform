use axum::{extract::Json, extract::Path, http::StatusCode, response::IntoResponse};
use tokio::time::Duration;
use tracing::{error, info};
use uuid::Uuid;

use crate::executor;
use crate::state::JOB_STORE;
use crate::types::{ExecutionRequest, ExecutionResult};
use crate::state::BROADCASTS;
use axum::extract::ws::{WebSocketUpgrade, WebSocket, Message};
use crate::state::RUNNING_CHILDREN;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn execute_handler(Json(req): Json<ExecutionRequest>) -> impl IntoResponse {
    // synchronous/blocking execution (keeps previous behavior)
    let id = Uuid::new_v4().to_string();
    info!(id = %id, language = %req.language, "starting execution");

    // basic validation
    let max_code: usize = std::env::var("MAX_CODE_BYTES").ok().and_then(|s| s.parse().ok()).unwrap_or(10_000);
    if req.code.len() > max_code {
        let body = ExecutionResult {
            id: id.clone(),
            status: "failed".into(),
            stdout: "".into(),
            stderr: "code too large".into(),
            exit_code: None,
        };
        return (StatusCode::BAD_REQUEST, Json(body));
    }

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
    // validation
    let max_code: usize = std::env::var("MAX_CODE_BYTES").ok().and_then(|s| s.parse().ok()).unwrap_or(10_000);
    if req.code.len() > max_code {
        let body = serde_json::json!({"error": "code too large"});
        return (StatusCode::BAD_REQUEST, Json(body));
    }

    // mark as running
    JOB_STORE.insert(id.clone(), None);

    let code = req.code.clone();
    let lang = req.language.clone();
    let timeout_dur = Duration::from_millis(req.timeout_ms.unwrap_or(5000));
    let id_clone = id.clone();

    tokio::spawn(async move {
        match lang.as_str() {
            "python" | "py" => {
                let _ = executor::run_python_stream(id_clone.clone(), &code, timeout_dur).await;
            }
            _ => {
                let body = ExecutionResult {
                    id: id_clone.clone(),
                    status: "failed".into(),
                    stdout: "".into(),
                    stderr: "unsupported language".into(),
                    exit_code: None,
                };
                JOB_STORE.insert(id_clone, Some(body));
            }
        }
    });

    let status_url = format!("/status/{}", id);
    let resp = serde_json::json!({
        "id": id,
        "status": "scheduled",
        "status_url": status_url
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

pub async fn list_jobs() -> impl IntoResponse {
    let mut list = Vec::new();
    for r in JOB_STORE.iter() {
        let id = r.key().clone();
        let status = match r.value() {
            None => "running",
            Some(res) => &res.status,
        };
        list.push(serde_json::json!({"id": id, "status": status}));
    }
    (StatusCode::OK, Json(serde_json::json!({"jobs": list})))
}

pub async fn cancel_job(Path(id): Path<String>) -> impl IntoResponse {
    // try to kill running child
    if let Some(slot) = RUNNING_CHILDREN.get(&id) {
        let mut guard = slot.lock().await;
        if let Some(child) = guard.as_mut() {
            let _ = child.kill().await;
            *guard = None;
        }
        JOB_STORE.insert(id.clone(), Some(ExecutionResult { id: id.clone(), status: "cancelled".into(), stdout: "".into(), stderr: "cancelled by user".into(), exit_code: None }));
        RUNNING_CHILDREN.remove(&id);
        return (StatusCode::OK, Json(serde_json::json!({"id": id, "status": "cancelled"}))); 
    }

    (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not running"})))
}

pub async fn metrics() -> impl IntoResponse {
    let total = JOB_STORE.len();
    let running = JOB_STORE.iter().filter(|r| r.value().is_none()).count();
    let completed = JOB_STORE.iter().filter(|r| r.value().is_some()).count();
    let resp = serde_json::json!({"total": total, "running": running, "completed": completed});
    (StatusCode::OK, Json(resp))
}

pub async fn ws_handler(ws: WebSocketUpgrade, Path(id): Path<String>) -> impl IntoResponse {
    ws.on_upgrade(move |mut socket: WebSocket| async move {
        // subscribe to broadcasts for this id
        if let Some(tx) = BROADCASTS.get(&id) {
            let mut rx = tx.subscribe();
            while let Ok(msg) = rx.recv().await {
                if socket.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
        } else {
            let _ = socket.send(Message::Text("no job or no logs".into())).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use axum::body;
    use serde_json::Value;
    use std::time::{Duration as StdDuration, Instant};

    #[tokio::test]
    async fn test_execute_handler_sync() {
        let req = ExecutionRequest {
            language: "python".into(),
            code: "print('sync-test')".into(),
            timeout_ms: Some(2000),
        };

        let resp = execute_handler(Json(req)).await.into_response();
        let status = resp.status();
        assert_eq!(status, StatusCode::OK);
        let body_bytes = body::to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        let v: ExecutionResult = serde_json::from_slice(&body_bytes).unwrap();
        assert!(v.stdout.contains("sync-test"));
    }

    #[tokio::test]
    async fn test_execute_async_flow() {
        let req = ExecutionRequest {
            language: "python".into(),
            code: "print('async-test')".into(),
            timeout_ms: Some(2000),
        };

        // schedule
        let resp = execute_async_handler(Json(req)).await.into_response();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let body_bytes = body::to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        let v: Value = serde_json::from_slice(&body_bytes).unwrap();
        let id = v.get("id").and_then(|s| s.as_str()).expect("id present").to_string();

        // poll status until completed (with timeout)
        let start = Instant::now();
        loop {
            if start.elapsed() > StdDuration::from_secs(5) {
                panic!("job did not complete in time");
            }
            let status_resp = status_handler(axum::extract::Path(id.clone())).await.into_response();
            let status_bytes = body::to_bytes(status_resp.into_body(), 64 * 1024).await.unwrap();
            let status_val: Value = serde_json::from_slice(&status_bytes).unwrap();
            if status_val.get("status").and_then(|s| s.as_str()) == Some("running") {
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                continue;
            }
            // must be completed or failed
            assert!(status_val.get("status").is_some());
            assert!(status_val.get("id").is_some());
            break;
        }
    }
}
