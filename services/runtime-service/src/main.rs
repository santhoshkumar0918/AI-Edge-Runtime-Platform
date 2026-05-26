use axum::{routing::{get, post}, Router, Extension, middleware};
use std::sync::Arc;
use std::collections::HashSet;
use axum::http::Request;
use axum::response::IntoResponse;
use axum::middleware::Next;
use std::net::SocketAddr;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod executor;
mod types;
mod state;
mod handlers {
    pub mod execute;
}

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    // load API keys from env `API_KEYS` (comma-separated)
    let keys_env = std::env::var("API_KEYS").unwrap_or_default();
    let keys: HashSet<String> = keys_env
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let keys = Arc::new(keys);

    async fn auth(req: Request<axum::body::Body>, next: Next) -> impl IntoResponse {
        // allow status endpoint without auth
        let path = req.uri().path().to_string();
        if path.starts_with("/status") {
            return next.run(req).await;
        }

        // get keys from extensions
        let allowed = req.extensions().get::<Arc<HashSet<String>>>().cloned();
        if allowed.is_none() {
            return (axum::http::StatusCode::UNAUTHORIZED, "unauthorized").into_response();
        }
        let allowed = allowed.unwrap();

        if let Some(hdr) = req.headers().get(axum::http::header::AUTHORIZATION) {
            if let Ok(s) = hdr.to_str() {
                if s.starts_with("Bearer ") {
                    let tok = s[7..].trim();
                    if allowed.contains(tok) {
                        return next.run(req).await;
                    }
                }
            }
        }

        (axum::http::StatusCode::UNAUTHORIZED, "unauthorized").into_response()
    }

    let app = Router::new()
        .route("/execute", post(handlers::execute::execute_handler))
        .route("/execute_async", post(handlers::execute::execute_async_handler))
        .route("/status/:id", get(handlers::execute::status_handler))
        .route("/ws/:id", get(handlers::execute::ws_handler))
        .route("/healthz", get(|| async { StatusCode::OK }));

    let app = app.layer(Extension(keys)).layer(middleware::from_fn(auth));

    let addr = SocketAddr::from(([127, 0, 0, 1], 8081));
    info!(%addr, "runtime-service listening");

    // Use axum-server to run the app (compatible with axum 0.8)
    axum_server::bind(addr).serve(app.into_make_service()).await.unwrap();
}
