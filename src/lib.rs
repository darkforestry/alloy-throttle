use std::{
    num::NonZeroU32,
    sync::Arc,
    task::{Context, Poll},
};

use alloy_json_rpc::{RequestPacket, ResponsePacket};
use alloy_transport::{TransportError, TransportFut};
use governor::{
    clock::{QuantaClock, QuantaInstant},
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Jitter, Quota, RateLimiter,
};

use thiserror::Error;
use tower::{Layer, Service};

pub type Throttle =
    RateLimiter<NotKeyed, InMemoryState, QuantaClock, NoOpMiddleware<QuantaInstant>>;

pub struct ThrottleLayer {
    throttle: Arc<Throttle>,
    jitter: Option<Jitter>,
}

#[derive(Debug, Error)]
pub enum ThrottleError {
    #[error("Requests per second must be a non-zero positive integer")]
    InvalidRequestsPerSecond,
}

impl ThrottleLayer {
    pub fn new(requests_per_second: u32, jitter: Option<Jitter>) -> Result<Self, ThrottleError> {
        let quota = NonZeroU32::new(requests_per_second)
            .ok_or(ThrottleError::InvalidRequestsPerSecond)
            .map(Quota::per_second)?;

        let throttle = Arc::new(RateLimiter::direct(quota));

        Ok(ThrottleLayer { throttle, jitter })
    }
}

impl<S> Layer<S> for ThrottleLayer {
    type Service = ThrottleService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ThrottleService {
            inner,
            throttle: self.throttle.clone(),
            jitter: self.jitter,
        }
    }
}

/// A Tower Service used by the ThrottleLayer that is responsible for throttling rpc requests.
#[derive(Debug, Clone)]
pub struct ThrottleService<S> {
    /// The inner service
    inner: S,
    throttle: Arc<Throttle>,
    jitter: Option<Jitter>,
}

impl<S> Service<RequestPacket> for ThrottleService<S>
where
    S: Service<RequestPacket, Response = ResponsePacket, Error = TransportError>
        + Send
        + 'static
        + Clone,
    S::Future: Send + 'static,
{
    type Response = ResponsePacket;
    type Error = TransportError;
    type Future = TransportFut<'static>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: RequestPacket) -> Self::Future {
        let throttle = self.throttle.clone();
        let jitter = self.jitter;
        let mut inner = self.inner.clone();

        Box::pin(async move {
            if let Some(jitter) = jitter {
                throttle.until_ready_with_jitter(jitter).await;
            } else {
                throttle.until_ready().await;
            }

            inner.call(request).await
        })
    }
}
