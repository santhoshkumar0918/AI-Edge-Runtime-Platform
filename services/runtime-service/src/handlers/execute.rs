use axum::{extract::Json, extract::Path, extract::Query, http::StatusCode, response::IntoResponse};
use tokio::time::{Duration, Instant};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::executor;
use crate::state::{JOB_STORE, METRICS};
use crate::types::{ExecutionRequest, ExecutionResult};
use crate::state::BROADCASTS;
use axum::extract::ws::{WebSocketUpgrade, WebSocket, Message};
use crate::state::RUNNING_CHILDREN;

pub async fn execute_handler(Json(req): Json<ExecutionRequest>) -> impl IntoResponse {
    // synchronous/blocking execution (keeps previous behavior)
    let id = Uuid::new_v4().to_string();
    let start = Instant::now();
    METRICS.record_job_started();
    info!(id = %id, language = %req.language, "starting execution");

    // validation: non-empty, size limits
    if req.code.trim().is_empty() {
        let now = chrono::Utc::now().timestamp_millis();
        let body = ExecutionResult { id: id.clone(), status: "failed".into(), stdout: "".into(), stderr: "empty code".into(), exit_code: None, created_at: Some(now) };
        crate::state::JOB_META.insert(id.clone(), now);
        crate::state::JOB_STORE.insert(id.clone(), Some(body.clone()));
        return (StatusCode::BAD_REQUEST, Json(body));
    }

    let max_code: usize = std::env::var("MAX_CODE_BYTES").ok().and_then(|s| s.parse().ok()).unwrap_or(10_000);
    if req.code.len() > max_code {
        let now = chrono::Utc::now().timestamp_millis();
        let body = ExecutionResult { id: id.clone(), status: "failed".into(), stdout: "".into(), stderr: "code too large".into(), exit_code: None, created_at: Some(now) };
        crate::state::JOB_META.insert(id.clone(), now);
        crate::state::JOB_STORE.insert(id.clone(), Some(body.clone()));
        return (StatusCode::BAD_REQUEST, Json(body));
    }

    // validate timeout
    let max_timeout_ms: u64 = std::env::var("MAX_TIMEOUT_MS").ok().and_then(|s| s.parse().ok()).unwrap_or(30_000);
    let requested = req.timeout_ms.unwrap_or(5_000) as u64;
    if requested == 0 || requested > max_timeout_ms {
        let now = chrono::Utc::now().timestamp_millis();
        let body = ExecutionResult { id: id.clone(), status: "failed".into(), stdout: "".into(), stderr: format!("invalid timeout; must be 1..{} ms", max_timeout_ms), exit_code: None, created_at: Some(now) };
        crate::state::JOB_META.insert(id.clone(), now);
        crate::state::JOB_STORE.insert(id.clone(), Some(body.clone()));
        return (StatusCode::BAD_REQUEST, Json(body));
    }

    let timeout_dur = Duration::from_millis(requested);

    // dispatch to executor
    let res = match req.language.as_str() {
        "python" | "py" => executor::run_python(&req.code, timeout_dur).await,
        _ => Err(anyhow::anyhow!("unsupported language")),
    };

    let elapsed_ms = start.elapsed().as_millis() as u64;
    match res {
        Ok((stdout, stderr, exit_code)) => {
            METRICS.record_job_completed(elapsed_ms);
            let now = chrono::Utc::now().timestamp_millis();
            let body = ExecutionResult {
                id: id.clone(),
                status: "completed".into(),
                stdout,
                stderr,
                exit_code,
                created_at: Some(now),
            };
            crate::state::JOB_META.insert(id.clone(), now);
            crate::state::JOB_STORE.insert(id.clone(), Some(body.clone()));
            info!(id = %id, elapsed_ms = elapsed_ms, "execution completed successfully");
            (StatusCode::OK, Json(body))
        }
        Err(e) => {
            METRICS.record_job_failed();
            error!(%e, id = %id, "execution failed");
            let now = chrono::Utc::now().timestamp_millis();
            let body = ExecutionResult {
                id: id.clone(),
                status: "failed".into(),
                stdout: "".into(),
                stderr: format!("{}", e),
                exit_code: None,
                created_at: Some(now),
            };
            crate::state::JOB_META.insert(id.clone(), now);
            crate::state::JOB_STORE.insert(id.clone(), Some(body.clone()));
            (StatusCode::INTERNAL_SERVER_ERROR, Json(body))
        }
    }
}

pub async fn execute_async_handler(Json(req): Json<ExecutionRequest>) -> impl IntoResponse {
    let id = Uuid::new_v4().to_string();
    info!(id = %id, language = %req.language, "scheduling async execution");

    // basic validation
    if req.code.trim().is_empty() {
        let body = serde_json::json!({"error": "empty code"});
        return (StatusCode::BAD_REQUEST, Json(body));
    }
    let max_code: usize = std::env::var("MAX_CODE_BYTES").ok().and_then(|s| s.parse().ok()).unwrap_or(10_000);
    if req.code.len() > max_code {
        let body = serde_json::json!({"error": "code too large"});
        return (StatusCode::BAD_REQUEST, Json(body));
    }

    let max_timeout_ms: u64 = std::env::var("MAX_TIMEOUT_MS").ok().and_then(|s| s.parse().ok()).unwrap_or(30_000);
    let requested = req.timeout_ms.unwrap_or(5_000) as u64;
    if requested == 0 || requested > max_timeout_ms {
        let body = serde_json::json!({"error": format!("invalid timeout; must be 1..{} ms", max_timeout_ms)});
        return (StatusCode::BAD_REQUEST, Json(body));
    }

    // mark as running
    JOB_STORE.insert(id.clone(), None);
    let now = chrono::Utc::now().timestamp_millis();
    crate::state::JOB_META.insert(id.clone(), now);

    let code = req.code.clone();
    let lang = req.language.clone();
    let timeout_dur = Duration::from_millis(requested);
    let id_clone = id.clone();

    tokio::spawn(async move {
        match lang.as_str() {
            "python" | "py" => {
                let _ = executor::run_python_stream(id_clone.clone(), &code, timeout_dur).await;
            }
            _ => {
                let now = chrono::Utc::now().timestamp_millis();
                let body = ExecutionResult { id: id_clone.clone(), status: "failed".into(), stdout: "".into(), stderr: "unsupported language".into(), exit_code: None, created_at: Some(now) };
                crate::state::JOB_META.insert(id_clone.clone(), now);
                JOB_STORE.insert(id_clone, Some(body));
            }
        }
    });

    let status_url = format!("/status/{}", id);
    let resp = serde_json::json!({ "id": id, "status": "scheduled", "status_url": status_url });

    (StatusCode::ACCEPTED, Json(resp))
}

pub async fn status_handler(Path(id): Path<String>) -> impl IntoResponse {
    if let Some(entry) = JOB_STORE.get(&id) {
        match entry.value() {
            None => {
                let created = crate::state::JOB_META.get(&id).map(|v| *v.value());
                (StatusCode::ACCEPTED, Json(serde_ json::json!({"id": id, "status": "running", "created_at": created})))
            }
            Some(res) => (StatusCode::OK, Json(serde_json::to_value(res.clone()).unwrap())),
        }
    } else {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not found"})))
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ListJobsQuery {
    status: Option<String>,
}

pub async fn list_jobs(Query(q): Query<ListJobsQuery>) -> impl IntoResponse {
    let mut list = Vec::new();
    for r in JOB_STORE.iter() {
        let id = r.key().clone();
        let status = match r.value() {
            None => "running".to_string(),
            Some(res) => res.status.clone(),
        };
        if let Some(filter) = &q.status {
            if filter != &status {
                continue;
            }
        }
        // try to read created_at from the stored result or from JOB_META
        let created_at = if let Some(res) = r.value() {
            res.created_at
        } else {
            crate::state::JOB_META.get(&id).map(|v| *v.value())
        };
        list.push(serde_json::json!({"id": id, "status": status, "created_at": created_at}));
    }
    (StatusCode::OK, Json(serde_json::json!({"jobs": list})))
}

pub async fn cancel_job(Path(id): Path<String>) -> impl IntoResponse {
    // try to kill running child
    if let Some(slot) = RUNNING_CHILDREN.get(&id) {
        let mut guard = slot.lock().await;
        if let Some(child) = guard.as_mut() {
            match child.kill().await {
                Ok(_) => info!(id = %id, "job cancelled successfully"),
                Err(e) => warn!(id = %id, error = %e, "error killing job process"),
            }
            *guard = None;
        }
        METRICS.record_job_cancelled();
        let created_at = crate::state::JOB_META.get(&id).map(|v| *v.value());
        JOB_STORE.insert(id.clone(), Some(ExecutionResult { id: id.clone(), status: "cancelled".into(), stdout: "".into(), stderr: "cancelled by user".into(), exit_code: None, created_at }));
        RUNNING_CHILDREN.remove(&id);
        return (StatusCode::OK, Json(serde_json::json!({"id": id, "status": "cancelled"})));
    }

    (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not running"})))
}

pub async fn metrics() -> impl IntoResponse {
    let snapshot = METRICS.snapshot();
    let avg_execution_time_ms = if snapshot.completed_jobs > 0 {
        snapshot.total_execution_time_ms / snapshot.completed_jobs
    } else {
        0
    };
    let success_rate = if snapshot.total_jobs > 0 {
        (snapshot.completed_jobs as f64 / snapshot.total_jobs as f64 * 100.0) as u64
    } else {
        0
    };
    let webhook_success_rate = if snapshot.webhook_delivered + snapshot.webhook_failed > 0 {
        (snapshot.webhook_delivered as f64 / (snapshot.webhook_delivered + snapshot.webhook_failed) as f64 * 100.0) as u64
    } else {
        0
    };
    let resp = serde_json::json!({
        "total_jobs": snapshot.total_jobs,
        "completed_jobs": snapshot.completed_jobs,
        "failed_jobs": snapshot.failed_jobs,
        "running_jobs": snapshot.running_jobs,
        "cancelled_jobs": snapshot.cancelled_jobs,
        "success_rate_pct": success_rate,
        "execution_time": {
            "total_ms": snapshot.total_execution_time_ms,
            "min_ms": if snapshot.min_execution_time_ms == u64::MAX { 0 } else { snapshot.min_execution_time_ms },
            "max_ms": snapshot.max_execution_time_ms,
            "avg_ms": avg_execution_time_ms,
        },
        "webhooks": {
            "delivered": snapshot.webhook_delivered,
            "failed": snapshot.webhook_failed,
            "success_rate_pct": webhook_success_rate,
        },
    });
    (StatusCode::OK, Json(resp))
}

pub async fn public_summary() -> impl IntoResponse {
    let total = JOB_STORE.len();
    let running = JOB_STORE.iter().filter(|r| r.value().is_none()).count();
    let completed = JOB_STORE.iter().filter(|r| r.value().is_some()).count();
    // compute latest job timestamp if available
    let mut latest: Option<i64> = None;
    for m in crate::state::JOB_META.iter() {
        let v = *m.value();
        latest = Some(latest.map_or(v, |cur| std::cmp::max(cur, v)));
    }
    let resp = serde_json::json!({
        "service": "runtime-service",
        "status": "ok",
        "total_jobs": total,
        "running_jobs": running,
        "completed_jobs": completed,
        "latest_job_at": latest,
    });
    (StatusCode::OK, Json(resp))
}

#[derive(Debug, serde::Deserialize)]
pub struct LogsQuery {
    tail: Option<usize>,
}
pub async fn get_job_logs(Path(id): Path<String>, Query(q): Query<LogsQuery>) -> impl IntoResponse {
    if let Some(entry) = JOB_STORE.get(&id) {
        match entry.value() {
            None => (StatusCode::ACCEPTED, Json(serde_json::json!({"id": id, "status": "running"}))),
            Some(res) => {
                let tail = q.tail.unwrap_or(0);
                let stdout = if tail == 0 { res.stdout.clone() } else { take_last_lines(&res.stdout, tail) };
                let stderr = if tail == 0 { res.stderr.clone() } else { take_last_lines(&res.stderr, tail) };
                (StatusCode::OK, Json(serde_json::json!({"id": id, "stdout": stdout, "stderr": stderr})))
            }
        }
    } else {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not found"})))
    }
}

fn take_last_lines(s: &str, n: usize) -> String {
    if n == 0 {
        return s.to_string();
    }
    let lines: Vec<&str> = s.lines().collect();
    let len = lines.len();
    let start = if n >= len { 0 } else { len - n };
    lines[start..].join("\n")
}
pub async fn purge_jobs() -> impl IntoResponse {
    for r in RUNNING_CHILDREN.iter() {
        let slot = r.value().clone();
        let mut guard = slot.lock().await;
        if let Some(child) = guard.as_mut() {
            let _ = child.kill().await;
        }
    }
    JOB_STORE.clear();
    BROADCASTS.clear();
    RUNNING_CHILDREN.clear();
    (StatusCode::OK, Json(serde_json::json!({"status": "purged"})))
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
