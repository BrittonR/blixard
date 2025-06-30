//! Authentication interceptor for gRPC services
//!
//! This module provides Tower middleware for authenticating gRPC requests
//! using token-based authentication.

use crate::security::SecurityManager;
use std::sync::Arc;
use tonic::{Request, Status};
use tower::{Layer, Service};
use std::task::{Context, Poll};
use futures::future::BoxFuture;

/// Authentication layer for Tower middleware stack
#[derive(Clone)]
pub struct AuthLayer {
    security_manager: Arc<SecurityManager>,
}

impl AuthLayer {
    /// Create a new authentication layer
    pub fn new(security_manager: Arc<SecurityManager>) -> Self {
        Self { security_manager }
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthService {
            inner,
            security_manager: self.security_manager.clone(),
        }
    }
}

/// Authentication service that wraps the inner service
#[derive(Clone)]
pub struct AuthService<S> {
    inner: S,
    security_manager: Arc<SecurityManager>,
}

impl<S, B> Service<Request<B>> for AuthService<S>
where
    S: Service<Request<B>, Response = tonic::Response<B>, Error = Status> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let security_manager = self.security_manager.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Extract token from metadata
            let token = req.metadata()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .ok_or_else(|| Status::unauthenticated("Missing or invalid authorization header"))?;

            // Validate token
            match security_manager.authenticate_token(token).await {
                Ok(auth_result) if auth_result.authenticated => {
                    // Token is valid, proceed with the request
                    // TODO: Add user context to request extensions for RBAC checks
                    inner.call(req).await
                }
                _ => {
                    Err(Status::unauthenticated("Invalid authentication token"))
                }
            }
        })
    }
}

/// Extract auth token from request metadata
pub fn extract_auth_token<T>(request: &Request<T>) -> Option<&str> {
    request.metadata()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

/// Macro to require authentication in gRPC handlers
#[macro_export]
macro_rules! require_auth {
    ($request:expr, $security_manager:expr) => {{
        let token = $crate::auth_interceptor::extract_auth_token(&$request)
            .ok_or_else(|| tonic::Status::unauthenticated("Missing authorization header"))?;
        
        let auth_result = $security_manager.authenticate_token(token).await
            .map_err(|_| tonic::Status::unauthenticated("Invalid authentication token"))?;
        
        if !auth_result.authenticated {
            return Err(tonic::Status::unauthenticated("Authentication failed"));
        }
        
        auth_result
    }};
}