use actix_web::{
    Error,
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
};
use futures_util::future::{LocalBoxFuture, Ready, ok};
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::Instant;

use crate::metrics::HTTP_LATENCY_MS;

pub static REAL_LAT_SUM_US: AtomicU64 = AtomicU64::new(0);
pub static REAL_LAT_COUNT: AtomicU64 = AtomicU64::new(0);

pub struct RealLatency;

impl<S, B> Transform<S, ServiceRequest> for RealLatency
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = RealLatencyMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(RealLatencyMiddleware { service })
    }
}

pub struct RealLatencyMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for RealLatencyMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(ctx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let start = Instant::now();
        let fut = self.service.call(req);

        Box::pin(async move {
            let res = fut.await?;
            let elapsed = start.elapsed().as_micros() as u64;

            REAL_LAT_SUM_US.fetch_add(elapsed, Ordering::Relaxed);
            REAL_LAT_COUNT.fetch_add(1, Ordering::Relaxed);
            HTTP_LATENCY_MS.observe(elapsed as f64 / 1000.0);

            Ok(res)
        })
    }
}
