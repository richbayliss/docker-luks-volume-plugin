use actix_http::error::{PayloadError, ResponseError};
use actix_http::Payload;
use actix_web::dev::Decompress;
use actix_web::{FromRequest, HttpRequest, HttpResponse};
use derive_more::{Display, From};
use futures::future::Future;
use futures::stream::Stream;
use futures::Poll;
use serde::de::DeserializeOwned;
use serde_json::error::Error as JsonError;

#[derive(Debug, Display, From)]
pub enum RpcRequestError {
    Broken,
    Deserialize(JsonError),
    Payload(PayloadError),
}

impl ResponseError for RpcRequestError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::new(actix_web::http::StatusCode::BAD_REQUEST)
    }
}

impl From<actix_http::error::Error> for RpcRequestError {
    fn from(_error: actix_http::error::Error) -> Self {
        RpcRequestError::Broken
    }
}

pub struct RpcRequest<T>(pub T);

impl<T> Default for RpcRequest<T>
where
    T: Default,
{
    fn default() -> Self {
        Self { 0: T::default() }
    }
}

#[derive(Clone)]
pub struct RpcRequestConfig {}
impl Default for RpcRequestConfig {
    fn default() -> Self {
        Self {}
    }
}

impl<T> FromRequest for RpcRequest<T>
where
    T: Default + DeserializeOwned + 'static,
{
    type Config = RpcRequestConfig;
    type Error = RpcRequestError;
    type Future = Box<dyn Future<Item = Self, Error = RpcRequestError>>;
    #[inline]
    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let req2 = req.clone();

        Box::new(
            RpcBody::new(req, payload)
                .map_err(move |e| {
                    log::debug!(
                        "Failed to deserialize Json from payload. \
                         Request path: {}",
                        req2.path()
                    );
                    e.into()
                })
                .map(RpcRequest),
        )
    }
}

pub struct RpcBody<U> {
    stream: Option<Decompress<Payload>>,
    err: Option<PayloadError>,
    fut: Option<Box<dyn Future<Item = U, Error = PayloadError>>>,
}

impl<U> RpcBody<U>
where
    U: DeserializeOwned + 'static,
{
    pub fn new(req: &HttpRequest, payload: &mut Payload) -> Self {
        let payload = Decompress::from_headers(payload.take(), req.headers());

        RpcBody {
            stream: Some(payload),
            err: None,
            fut: None,
        }
    }
}

impl<U> Future for RpcBody<U>
where
    U: DeserializeOwned + 'static,
{
    type Item = U;
    type Error = PayloadError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some(ref mut fut) = self.fut {
            return fut.poll();
        }

        if let Some(err) = self.err.take() {
            return Err(err);
        }

        self.fut = Some(Box::new(
            self.stream
                .take()
                .unwrap()
                .concat2()
                .from_err()
                .and_then(move |body| {
                    let payload = match String::from_utf8(body.to_vec()) {
                        Ok(v) => v,
                        Err(_) => "".to_string(),
                    };

                    serde_json::from_str(&payload).map_err(|_| PayloadError::Overflow)
                }), // self.stream
                    // .take()
                    // .unwrap()
                    // .from_err()
                    // .fold(BytesMut::with_capacity(8192), move |mut body, chunk| {
                    //     body.extend_from_slice(&chunk);
                    //     Ok(body)
                    // })
                    // .and_then(|body| serde_json::from_slice::<U>(body))
        ));
        self.poll()
    }
}
