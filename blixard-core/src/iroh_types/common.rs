//! Common response wrapper and shared types for Iroh transport

/// Simple Response wrapper to replace tonic::Response
#[derive(Debug, Clone)]
pub struct Response<T> {
    inner: T,
}

impl<T> Response<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    pub fn get_ref(&self) -> &T {
        &self.inner
    }
}