use axum::{routing::post, Router};
use std::net::SocketAddr;
use tracing::info;
use tracing_subscriber::EnvFilter;
use axum::Server;

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
    let std_listener = std::net::TcpListener::bind(addr).expect("bind tcp");
    std_listener
        .set_nonblocking(true)
        .expect("set nonblocking");
    let server = hyper::Server::from_tcp(std_listener)
        .expect("from_tcp")
        .serve(app.into_make_service());
    server.await.unwrap();
}
