//! Static file server for the built front end, plus `/health`.

use std::{env, net::SocketAddr, path::PathBuf};

use axum::{
    Router,
    extract::Request,
    http::{HeaderValue, Response, header},
    middleware::{map_request, map_response},
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
    // Vite's content-hashed files never change. The bundle and index.html are unhashed, and their
    // validators come from the Nix store (mtime 1, so only the size differs between builds): a
    // same-size index.html after a deploy would be answered 304 with the old asset names. They
    // are therefore always sent in full.
    // A missing file under /assets or /data is a real 404; only unmatched paths get the SPA shell.
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        // The link-preview image is a real file, not the SPA shell: Discord and the
        // other unfurlers fetch it without running JS (Anubis in front of this host
        // lets exactly /og.png through for that reason).
        .nest_service(
            "/og.png",
            ServiceBuilder::new()
                .layer(cache("public, max-age=86400"))
                .service(ServeFile::new(dir.join("og.png"))),
        )
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
                .layer(map_request(strip_conditions))
                .layer(map_response(strip_validators))
                .service(ServeDir::new(dir.join("data"))),
        )
        .fallback_service(
            ServiceBuilder::new()
                .layer(cache("no-cache"))
                .layer(map_request(strip_conditions))
                .layer(map_response(strip_validators))
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

/// Drop conditional request headers so the file service never answers 304.
async fn strip_conditions(mut req: Request) -> Request {
    let h = req.headers_mut();
    h.remove(header::IF_NONE_MATCH);
    h.remove(header::IF_MODIFIED_SINCE);
    req
}

/// Drop validators so browsers do not send conditional requests for unhashed files.
async fn strip_validators<B>(mut res: Response<B>) -> Response<B> {
    let h = res.headers_mut();
    h.remove(header::ETAG);
    h.remove(header::LAST_MODIFIED);
    res
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
