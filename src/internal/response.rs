use crate::body::Body;
use crate::error::Error;
use futures::channel::oneshot;
use futures::prelude::*;
use http::Response;
use std::pin::Pin;
use std::task::*;

// A future for a response.
pub struct ResponseFuture {
    completed: bool,
    receiver: oneshot::Receiver<Result<Response<Body>, Error>>,
}

impl ResponseFuture {
    pub fn new() -> (Self, ResponseProducer) {
        let (sender, receiver) = oneshot::channel();

        let future = Self {
            completed: false,
            receiver,
        };

        let producer = ResponseProducer {
            sender: Some(sender),
            status_code: None,
            version: None,
            headers: http::HeaderMap::new(),
        };

        (future, producer)
    }
}

impl Future for ResponseFuture {
    type Output = Result<Response<Body>, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let inner = Pin::new(&mut self.receiver);

        match inner.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => {
                self.completed = true;
                match result {
                    Ok(result) => Poll::Ready(result),
                    Err(oneshot::Canceled) => Poll::Ready(Err(Error::Aborted)),
                }
            },
        }
    }
}

impl Drop for ResponseFuture {
    fn drop(&mut self) {
        self.receiver.close();
        if !self.completed {
            log::debug!("request future canceled by user");
        }
    }
}

/// Producing end of a response future that builds up the response object
/// incrementally.
///
/// If dropped before the response is finished, the associated future will be
/// completed with an `Aborted` error.
pub struct ResponseProducer {
    sender: Option<oneshot::Sender<Result<Response<Body>, Error>>>,

    /// Status code of the response.
    pub(crate) status_code: Option<http::StatusCode>,

    /// HTTP version of the response.
    pub(crate) version: Option<http::Version>,

    /// Response headers received so far.
    pub(crate) headers: http::HeaderMap,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ResponseState {
    Active,
    Canceled,
    Completed,
}

impl ResponseProducer {
    pub fn state(&self) -> ResponseState {
        match self.sender.as_ref() {
            Some(sender) => match sender.is_canceled() {
                true => ResponseState::Canceled,
                false => ResponseState::Active,
            },
            None => ResponseState::Completed,
        }
    }

    /// Finishes constructing the response and sends it to the receiver.
    pub fn finish(&mut self, body: Body) -> bool {
        let mut builder = http::Response::builder();
        builder.status(self.status_code.take().unwrap());
        builder.version(self.version.take().unwrap());

        for (name, values) in self.headers.drain() {
            for value in values {
                builder.header(&name, value);
            }
        }

        let response = builder
            .body(body)
            .unwrap();

        match self.sender.take() {
            Some(sender) => match sender.send(Ok(response)) {
                Ok(()) => true,
                Err(_) => {
                    log::info!("response future cancelled");
                    false
                },
            }
            None => {
                log::warn!("response future already completed!");
                false
            },
        }
    }

    pub fn complete_with_error(&mut self, error: impl Into<Error>) -> bool {
        match self.sender.take() {
            Some(sender) => match sender.send(Err(error.into())) {
                Ok(()) => true,
                Err(_) => {
                    log::info!("response future cancelled");
                    false
                },
            }
            None => {
                log::warn!("response future already completed!");
                false
            },
        }
    }
}

static_assertions::assert_impl!(f; ResponseFuture, Send);
