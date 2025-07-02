//! HTTP server for exposing Prometheus metrics
//!
//! This module provides a simple HTTP server that exposes metrics
//! at the /metrics endpoint for Prometheus scraping.

use std::net::SocketAddr;
use std::convert::Infallible;
use hyper::{Body, Request, Response, Server, StatusCode};
use hyper::service::{make_service_fn, service_fn};
use crate::error::{BlixardResult, BlixardError};
use crate::metrics_otel::prometheus_metrics;

/// Handle HTTP requests to the metrics server
async fn handle_request(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let response = match req.uri().path() {
        "/metrics" => {
            let metrics = prometheus_metrics();
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/plain; version=0.0.4")
                .body(Body::from(metrics))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Failed to build response"))
                        .expect("Failed to build error response")
                })
        }
        "/health" => {
            Response::builder()
                .status(StatusCode::OK)
                .body(Body::from("OK\n"))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Failed to build response"))
                        .expect("Failed to build error response")
                })
        }
        "/" => {
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/html")
                .body(Body::from(r#"<html>
<head><title>Blixard Metrics</title></head>
<body>
<h1>Blixard Metrics Server</h1>
<p>Available endpoints:</p>
<ul>
<li><a href="/metrics">/metrics</a> - Prometheus metrics</li>
<li><a href="/health">/health</a> - Health check</li>
</ul>
</body>
</html>"#))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Failed to build response"))
                        .expect("Failed to build error response")
                })
        }
        _ => {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("404 Not Found\n"))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Failed to build response"))
                        .expect("Failed to build error response")
                })
        }
    };
    
    Ok(response)
}

/// Start the metrics HTTP server
pub async fn start_metrics_server(bind_address: SocketAddr) -> BlixardResult<()> {
    let make_svc = make_service_fn(|_conn| async {
        Ok::<_, Infallible>(service_fn(handle_request))
    });

    let server = Server::bind(&bind_address).serve(make_svc);
    
    tracing::info!("Metrics server listening on http://{}", bind_address);
    
    if let Err(e) = server.await {
        tracing::error!("Metrics server error: {}", e);
        return Err(crate::error::BlixardError::Internal {
            message: format!("Metrics server failed: {}", e),
        });
    }
    
    Ok(())
}

/// Start the metrics server in a background task
pub fn spawn_metrics_server(bind_address: SocketAddr) -> tokio::task::JoinHandle<BlixardResult<()>> {
    tokio::spawn(async move {
        start_metrics_server(bind_address).await
    })
}