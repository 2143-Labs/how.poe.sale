//! Static file server for the built front end, plus `/health`.

use std::{env, net::SocketAddr, path::PathBuf};

use axum::{
    Router,
    http::{HeaderValue, header},
    routing::get,
};
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let port: u16 =
        env::var("PORT").map_or(8080, |p| p.parse().expect("PORT must be a port number"));
    let dir = PathBuf::from(env::var("STATIC_DIR").unwrap_or_else(|_| "./web/dist".into()));
    let cache = |v: &'static str| {
        SetResponseHeaderLayer::overriding(header::CACHE_CONTROL, HeaderValue::from_static(v))
    };
    // Vite's content-hashed files never change; the bundle and index.html are unhashed, so they revalidate.
    // A missing file under /assets or /data is a real 404; only unmatched paths get the SPA shell.
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .nest_service(
            "/assets",
            ServiceBuilder::new()
                .layer(cache("public, max-age=31536000, immutable"))
                .service(ServeDir::new(dir.join("assets"))),
        )
        .nest_service(
            "/data",
            ServiceBuilder::new()
                .layer(cache("no-cache"))
                .service(ServeDir::new(dir.join("data"))),
        )
        .fallback_service(
            ServiceBuilder::new()
                .layer(cache("no-cache"))
                .service(ServeFile::new(dir.join("index.html"))),
        )
        .layer(CompressionLayer::new());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    tracing::info!(%addr, static_dir = %dir.display(), "listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await
        .expect("serve");
}

/// The server is PID 1 in its container, where SIGTERM has no default action;
/// without a handler every rollout would wait out the 30 s grace period.
async fn shutdown() {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("install SIGTERM handler");
    tokio::select! {
        _ = term.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
    }
}
