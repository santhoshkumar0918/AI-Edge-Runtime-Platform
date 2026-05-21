use axum::{routing::post, Router};
use std::net::SocketAddr;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod executor;
mod handlers {
    pub mod execute;
}

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let app = Router::new().route("/execute", post(handlers::execute::execute_handler));

    let addr = SocketAddr::from(([127, 0, 0, 1], 8081));
    info!(%addr, "runtime-service listening");

    // Use axum-server to run the app (compatible with axum 0.8)
    axum_server::bind(addr).serve(app.into_make_service()).await.unwrap();
}
