//! HTTP server for exposing Prometheus metrics and bootstrap information
//!
//! This module provides a simple HTTP server that exposes metrics
//! at the /metrics endpoint for Prometheus scraping and P2P bootstrap
//! information at the /bootstrap endpoint.

use crate::error::BlixardResult;
use crate::iroh_types::BootstrapInfo;
#[cfg(feature = "observability")]
use crate::metrics_otel::prometheus_metrics;
use crate::node_shared::SharedNodeState;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

/// Handle HTTP requests to the metrics server
async fn handle_request(
    req: Request<Body>,
    shared_state: Arc<SharedNodeState>,
) -> Result<Response<Body>, Infallible> {
    let response = match req.uri().path() {
        "/metrics" => handle_metrics_endpoint(),
        "/health" => handle_health_endpoint(),
        "/bootstrap" => handle_bootstrap_endpoint(shared_state).await,
        "/" => handle_root_endpoint(),
        _ => handle_not_found(),
    };

    Ok(response)
}

/// Build a response or return internal server error
fn build_response(
    status: StatusCode,
    content_type: Option<&str>,
    body: String,
) -> Response<Body> {
    let mut builder = Response::builder().status(status);
    
    if let Some(ct) = content_type {
        builder = builder.header("Content-Type", ct);
    }
    
    builder.body(Body::from(body)).unwrap_or_else(|_| {
        // Create an absolutely minimal response that cannot fail
        // Using new() instead of builder() to avoid any potential failures
        let mut response = Response::new(Body::from("Internal Server Error"));
        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        response
    })
}

/// Handle /metrics endpoint
fn handle_metrics_endpoint() -> Response<Body> {
    #[cfg(feature = "observability")]
    let metrics = prometheus_metrics();
    #[cfg(not(feature = "observability"))]
    let metrics = "# Observability features disabled\n";

    build_response(
        StatusCode::OK,
        Some("text/plain; version=0.0.4"),
        metrics.to_string(),
    )
}

/// Handle /health endpoint
fn handle_health_endpoint() -> Response<Body> {
    build_response(StatusCode::OK, None, "OK\n".to_string())
}

/// Handle /bootstrap endpoint
async fn handle_bootstrap_endpoint(shared_state: Arc<SharedNodeState>) -> Response<Body> {
    match get_bootstrap_info(&shared_state).await {
        Ok(info) => match serde_json::to_string(&info) {
            Ok(json) => build_response(
                StatusCode::OK,
                Some("application/json"),
                json,
            ),
            Err(e) => build_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                None,
                format!("Failed to serialize bootstrap info: {}", e),
            ),
        },
        Err(e) => build_response(
            StatusCode::SERVICE_UNAVAILABLE,
            None,
            format!("Bootstrap info not available: {}", e),
        ),
    }
}

/// Handle root endpoint
fn handle_root_endpoint() -> Response<Body> {
    const INDEX_HTML: &str = r#"<html>
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
</html>"#;

    build_response(
        StatusCode::OK,
        Some("text/html"),
        INDEX_HTML.to_string(),
    )
}

/// Handle 404 Not Found
fn handle_not_found() -> Response<Body> {
    build_response(
        StatusCode::NOT_FOUND,
        None,
        "404 Not Found\n".to_string(),
    )
}

/// Get bootstrap information from shared state
async fn get_bootstrap_info(shared_state: &SharedNodeState) -> BlixardResult<BootstrapInfo> {
    // Get the Iroh endpoint information
    let endpoint = shared_state.get_iroh_endpoint().await.ok_or_else(|| {
        crate::error::BlixardError::NotInitialized {
            component: "Iroh endpoint".to_string(),
        }
    })?;

    // Get P2P node address if available
    let (p2p_node_id, p2p_addresses, p2p_relay_url) = 
        if let Some(node_addr) = shared_state.get_p2p_node_addr().await {
            let node_id_str = node_addr.node_id.to_string();
            let addresses: Vec<String> = node_addr.direct_addresses()
                .map(|addr| addr.to_string())
                .collect();
            let relay_url = node_addr.relay_url().map(|url| url.to_string());
            (node_id_str, addresses, relay_url)
        } else {
            // Fallback to endpoint info
            let node_id_str = endpoint.node_id().to_string();
            let addresses = if let Some(socket) = endpoint.bound_sockets().first() {
                vec![socket.to_string()]
            } else {
                vec!["127.0.0.1:0".to_string()]
            };
            (node_id_str, addresses, None)
        };

    Ok(BootstrapInfo {
        node_id: shared_state.get_id(),
        p2p_node_id,
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
    tokio::spawn(async move { start_metrics_server(bind_address, shared_state).await })
}
