use axum::{
    body::Bytes,
    extract::{Path, Query, State, ws::{WebSocketUpgrade, WebSocket, Message as AxumMessage, CloseFrame as AxumCloseFrame}},
    http::{HeaderMap, Method, StatusCode},
    response::IntoResponse,
    routing::{get, post, delete},
    Router,
};
use std::sync::Arc;
use tracing::{error, info, warn};
use reqwest::header::HeaderName;
use futures_util::{SinkExt, StreamExt};

#[derive(Clone)]
struct AppState {
    client: reqwest::Client,
    runtime_url: String,
    runtime_ws_url: String,
    runtime_api_key: String,
}

#[tokio::main]
async fn main() {
    // initialize tracing subscriber
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("api-gateway starting up");

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let runtime_url = std::env::var("RUNTIME_SERVICE_URL").unwrap_or_else(|_| "http://127.0.0.1:8081".to_string());
    let runtime_ws_url = std::env::var("RUNTIME_SERVICE_WS_URL").unwrap_or_else(|_| "ws://127.0.0.1:8081".to_string());
    let runtime_api_key = std::env::var("RUNTIME_API_KEY").unwrap_or_default();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(35))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let state = AppState {
        client,
        runtime_url,
        runtime_ws_url,
        runtime_api_key,
    };

    let app = Router::new()
        .route("/execute", post(execute_handler))
        .route("/execute_async", post(execute_async_handler))
        .route("/status/:id", get(status_handler))
        .route("/jobs", get(list_jobs_handler))
        .route("/jobs/:id/logs", get(get_job_logs_handler))
        .route("/jobs/:id", delete(cancel_job_handler))
        .route("/ws/:id", get(ws_handler))
        .route("/public/summary", get(public_summary_handler))
        .route("/healthz", get(healthz_handler))
        .with_state(state);

    // Add CORS middleware so frontend can communicate
    let app = app.layer(
        tower_http::cors::CorsLayer::permissive()
    );

    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind listener");
    info!(%addr, "api-gateway listening");

    axum::serve(listener, app).await.expect("run server");
}

async fn forward_request(
    state: &AppState,
    method: Method,
    path: &str,
    query_string: Option<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, StatusCode> {
    let mut url = format!("{}{}", state.runtime_url, path);
    if let Some(qs) = query_string {
        if !qs.is_empty() {
            url = format!("{}?{}", url, qs);
        }
    }

    let reqwest_method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .unwrap_or(reqwest::Method::GET);
    let mut req_builder = state.client.request(reqwest_method, &url).body(body);

    // Copy incoming headers, skipping Host and Authorization to avoid skews
    for (name, value) in headers.iter() {
        if name != axum::http::header::HOST && name != axum::http::header::AUTHORIZATION {
            if let Ok(reqwest_val) = reqwest::header::HeaderValue::from_bytes(value.as_bytes()) {
                req_builder = req_builder.header(name.as_str(), reqwest_val);
            }
        }
    }

    // Add API key authorization for runtime-service
    if !state.runtime_api_key.is_empty() {
        req_builder = req_builder.header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", state.runtime_api_key),
        );
    }

    match req_builder.send().await {
        Ok(resp) => {
            let status = StatusCode::from_u16(resp.status().as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let mut response_headers = HeaderMap::new();
            for (key, val) in resp.headers().iter() {
                if let Ok(name) = axum::http::HeaderName::from_bytes(key.as_str().as_bytes()) {
                    if let Ok(value) = axum::http::HeaderValue::from_bytes(val.as_bytes()) {
                        response_headers.insert(name, value);
                    }
                }
            }
            let body_bytes = resp.bytes().await.map_err(|e| {
                error!("failed to read response bytes: {}", e);
                StatusCode::BAD_GATEWAY
            })?;
            Ok((status, response_headers, body_bytes))
        }
        Err(e) => {
            error!("failed to forward request to runtime-service: {}", e);
            Err(StatusCode::BAD_GATEWAY)
        }
    }
}

async fn execute_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    forward_request(&state, Method::POST, "/execute", None, headers, body).await
}

async fn execute_async_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    forward_request(&state, Method::POST, "/execute_async", None, headers, body).await
}

async fn status_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    forward_request(&state, Method::GET, &format!("/status/{}", id), None, headers, Bytes::new()).await
}

async fn get_job_logs_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let qs = if !params.is_empty() {
        Some(serde_urlencoded::to_string(&params).unwrap_or_default())
    } else {
        None
    };
    forward_request(&state, Method::GET, &format!("/jobs/{}/logs", id), qs, headers, Bytes::new()).await
}

async fn list_jobs_handler(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let qs = if !params.is_empty() {
        Some(serde_urlencoded::to_string(&params).unwrap_or_default())
    } else {
        None
    };
    forward_request(&state, Method::GET, "/jobs", qs, headers, Bytes::new()).await
}

async fn cancel_job_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    forward_request(&state, Method::DELETE, &format!("/jobs/{}", id), None, headers, Bytes::new()).await
}

async fn public_summary_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    forward_request(&state, Method::GET, "/public/summary", None, headers, Bytes::new()).await
}

async fn healthz_handler() -> impl IntoResponse {
    (StatusCode::OK, axum::Json(serde_json::json!({"status": "ok", "service": "api-gateway"})))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, id, state))
}

async fn handle_socket(client_socket: WebSocket, id: String, state: AppState) {
    let runtime_ws_url = format!("{}/ws/{}", state.runtime_ws_url.trim_end_matches('/'), id);
    info!("connecting api-gateway websocket proxy to runtime-service at {}", runtime_ws_url);

    // Connect to runtime-service websocket
    let req = match tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(&runtime_ws_url) {
        Ok(mut r) => {
            // Add API key authorization for runtime-service websocket
            if !state.runtime_api_key.is_empty() {
                r.headers_mut().insert(
                    "Authorization",
                    format!("Bearer {}", state.runtime_api_key).parse().unwrap(),
                );
            }
            r
        }
        Err(e) => {
            error!("failed to create websocket request: {}", e);
            return;
        }
    };

    let (runtime_socket, _) = match tokio_tungstenite::connect_async(req).await {
        Ok(res) => res,
        Err(e) => {
            error!("failed to connect to runtime-service websocket: {}", e);
            return;
        }
    };

    let (mut client_write, mut client_read) = client_socket.split();
    let (mut runtime_write, mut runtime_read) = runtime_socket.split();

    // Spawn task to forward from runtime to client
    let mut runtime_to_client = tokio::spawn(async move {
        while let Some(Ok(msg)) = runtime_read.next().await {
            if let Some(axum_msg) = tungstenite_to_axum(msg) {
                if client_write.send(axum_msg).await.is_err() {
                    break;
                }
            }
        }
    });

    // Spawn task to forward from client to runtime
    let mut client_to_runtime = tokio::spawn(async move {
        while let Some(Ok(msg)) = client_read.next().await {
            if let Some(tung_msg) = axum_to_tungstenite(msg) {
                if runtime_write.send(tung_msg).await.is_err() {
                    break;
                }
            }
        }
    });

    // Wait for either forwarding direction to end
    tokio::select! {
        _ = &mut runtime_to_client => {
            client_to_runtime.abort();
        }
        _ = &mut client_to_runtime => {
            runtime_to_client.abort();
        }
    }
    info!("websocket proxy session completed for job {}", id);
}

fn tungstenite_to_axum(msg: tokio_tungstenite::tungstenite::Message) -> Option<AxumMessage> {
    match msg {
        tokio_tungstenite::tungstenite::Message::Text(t) => Some(AxumMessage::Text(t.as_str().into())),
        tokio_tungstenite::tungstenite::Message::Binary(b) => Some(AxumMessage::Binary(axum::body::Bytes::from(b.to_vec()))),
        tokio_tungstenite::tungstenite::Message::Ping(p) => Some(AxumMessage::Ping(axum::body::Bytes::from(p.to_vec()))),
        tokio_tungstenite::tungstenite::Message::Pong(p) => Some(AxumMessage::Pong(axum::body::Bytes::from(p.to_vec()))),
        tokio_tungstenite::tungstenite::Message::Close(c) => Some(AxumMessage::Close(c.map(|frame| AxumCloseFrame {
            code: frame.code.into(),
            reason: frame.reason.to_string().into(),
        }))),
        _ => None,
    }
}

fn axum_to_tungstenite(msg: AxumMessage) -> Option<tokio_tungstenite::tungstenite::Message> {
    match msg {
        AxumMessage::Text(t) => Some(tokio_tungstenite::tungstenite::Message::Text(t.as_str().into())),
        AxumMessage::Binary(b) => Some(tokio_tungstenite::tungstenite::Message::Binary(b.to_vec().into())),
        AxumMessage::Ping(p) => Some(tokio_tungstenite::tungstenite::Message::Ping(p.to_vec().into())),
        AxumMessage::Pong(p) => Some(tokio_tungstenite::tungstenite::Message::Pong(p.to_vec().into())),
        AxumMessage::Close(c) => Some(tokio_tungstenite::tungstenite::Message::Close(c.map(|frame| tokio_tungstenite::tungstenite::protocol::CloseFrame {
            code: frame.code.into(),
            reason: frame.reason.to_string().into(),
        }))),
    }
}
