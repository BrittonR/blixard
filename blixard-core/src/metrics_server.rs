//! HTTP server for exposing Prometheus metrics and bootstrap information
//!
//! This module provides a simple HTTP server that exposes metrics
//! at the /metrics endpoint for Prometheus scraping and P2P bootstrap
//! information at the /bootstrap endpoint.

use std::net::SocketAddr;
use std::convert::Infallible;
use std::sync::Arc;
use hyper::{Body, Request, Response, Server, StatusCode};
use hyper::service::{make_service_fn, service_fn};
use crate::error::{BlixardResult, BlixardError};
use crate::metrics_otel::prometheus_metrics;
use crate::node_shared::SharedNodeState;
use crate::iroh_types::BootstrapInfo;

/// Handle HTTP requests to the metrics server
async fn handle_request(
    req: Request<Body>,
    shared_state: Arc<SharedNodeState>,
) -> Result<Response<Body>, Infallible> {
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
        "/bootstrap" => {
            // Get P2P information from shared state
            match get_bootstrap_info(&shared_state).await {
                Ok(info) => {
                    match serde_json::to_string(&info) {
                        Ok(json) => {
                            Response::builder()
                                .status(StatusCode::OK)
                                .header("Content-Type", "application/json")
                                .body(Body::from(json))
                                .unwrap_or_else(|_| {
                                    Response::builder()
                                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                                        .body(Body::from("Failed to build response"))
                                        .expect("Failed to build error response")
                                })
                        }
                        Err(e) => {
                            Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Body::from(format!("Failed to serialize bootstrap info: {}", e)))
                                .expect("Failed to build error response")
                        }
                    }
                }
                Err(e) => {
                    Response::builder()
                        .status(StatusCode::SERVICE_UNAVAILABLE)
                        .body(Body::from(format!("Bootstrap info not available: {}", e)))
                        .expect("Failed to build error response")
                }
            }
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
<li><a href="/bootstrap">/bootstrap</a> - P2P bootstrap information</li>
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

/// Get bootstrap information from shared state
async fn get_bootstrap_info(shared_state: &SharedNodeState) -> BlixardResult<BootstrapInfo> {
    // Get the Iroh endpoint information
    let (endpoint, node_id) = shared_state.get_iroh_endpoint().await?;
    
    // Get our addresses
    let our_addrs = endpoint.bound_sockets();
    let p2p_addresses: Vec<String> = our_addrs
        .into_iter()
        .map(|addr| addr.to_string())
        .collect();
    
    // Get relay URL if available
    let p2p_relay_url = endpoint.home_relay()
        .map(|url| url.to_string());
    
    Ok(BootstrapInfo {
        node_id: shared_state.get_id(),
        p2p_node_id: node_id.to_string(),
        p2p_addresses,
        p2p_relay_url,
    })
}

/// Start the metrics HTTP server
pub async fn start_metrics_server(
    bind_address: SocketAddr,
    shared_state: Arc<SharedNodeState>,
) -> BlixardResult<()> {
    let make_svc = make_service_fn(move |_conn| {
        let shared_state = shared_state.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let shared_state = shared_state.clone();
                handle_request(req, shared_state)
            }))
        }
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
pub fn spawn_metrics_server(
    bind_address: SocketAddr,
    shared_state: Arc<SharedNodeState>,
) -> tokio::task::JoinHandle<BlixardResult<()>> {
    tokio::spawn(async move {
        start_metrics_server(bind_address, shared_state).await
    })
}