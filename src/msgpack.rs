use actix_web::{
    FromRequest, HttpRequest, HttpResponse,
    dev::Payload,
    error::{ErrorBadRequest, ErrorInternalServerError},
};
use bytes::BytesMut;
use futures::StreamExt;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fmt,
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
};

pub struct MsgPack<T>(pub T);

impl<T> MsgPack<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for MsgPack<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for MsgPack<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: fmt::Debug> fmt::Debug for MsgPack<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MsgPack: {:?}", self.0)
    }
}

impl<T: DeserializeOwned + 'static> FromRequest for MsgPack<T> {
    type Error = actix_web::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let mut payload = payload.take();

        Box::pin(async move {
            let mut body = BytesMut::new();

            while let Some(chunk) = payload.next().await {
                let chunk = chunk.map_err(ErrorBadRequest)?;
                body.extend_from_slice(&chunk);
            }

            let data = rmp_serde::from_slice::<T>(&body)
                .map_err(|e| ErrorBadRequest(format!("MessagePack deserialize error: {}", e)))?;

            Ok(MsgPack(data))
        })
    }
}

pub trait MsgPackResponse: Sized + Serialize {
    fn msgpack(self) -> HttpResponse {
        match rmp_serde::to_vec(&self) {
            Ok(body) => HttpResponse::Ok()
                .content_type("application/msgpack")
                .body(body),
            Err(e) => HttpResponse::InternalServerError()
                .body(format!("MessagePack serialization error: {}", e)),
        }
    }
}

impl<T: Serialize> MsgPackResponse for T {}
