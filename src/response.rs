use crate::{context::RequestContext, metrics::Metrics, redirect::EffectiveUri};
use futures_lite::io::{AsyncRead, AsyncWrite};
use http::{Response, Uri};
use std::{
    fs::File,
    future::Future,
    io::{self, Read, Write},
    net::SocketAddr,
    path::Path,
    pin::Pin,
    task::{Context, Poll},
};

/// Provides extension methods for working with HTTP responses.
pub trait ResponseExt<T> {
    /// Get the effective URI of this response. This value differs from the
    /// original URI provided when making the request if at least one redirect
    /// was followed.
    ///
    /// This information is only available if populated by the HTTP client that
    /// produced the response.
    fn effective_uri(&self) -> Option<&Uri>;

    /// Get the local socket address of the last-used connection involved in
    /// this request, if known.
    ///
    /// Multiple connections may be involved in a request, such as with
    /// redirects.
    ///
    /// This method only makes sense with a normal Internet request. If some
    /// other kind of transport is used to perform the request, such as a Unix
    /// socket, then this method will return `None`.
    fn local_addr(&self) -> Option<SocketAddr>;

    /// Get the remote socket address of the last-used connection involved in
    /// this request, if known.
    ///
    /// Multiple connections may be involved in a request, such as with
    /// redirects.
    ///
    /// This method only makes sense with a normal Internet request. If some
    /// other kind of transport is used to perform the request, such as a Unix
    /// socket, then this method will return `None`.
    ///
    /// # Addresses and proxies
    ///
    /// The address returned by this method is the IP address and port that the
    /// client _connected to_ and not necessarily the real address of the origin
    /// server. Forward and reverse proxies between the caller and the server
    /// can cause the address to be returned to reflect the address of the
    /// nearest proxy rather than the server.
    fn remote_addr(&self) -> Option<SocketAddr>;

    /// Get the configured cookie jar used for persisting cookies from this
    /// response, if any.
    ///
    /// # Availability
    ///
    /// This method is only available when the [`cookies`](index.html#cookies)
    /// feature is enabled.
    #[cfg(feature = "cookies")]
    fn cookie_jar(&self) -> Option<&crate::cookies::CookieJar>;

    /// If request metrics are enabled for this particular transfer, return a
    /// metrics object containing a live view of currently available data.
    ///
    /// By default metrics are disabled and `None` will be returned. To enable
    /// metrics you can use
    /// [`Configurable::metrics`](crate::config::Configurable::metrics).
    fn metrics(&self) -> Option<&Metrics>;

    /// Abort this response transfer without reading any more of the response
    /// body from the server.
    ///
    /// This is usually undesirable behavior as it makes it impossible to reuse
    /// the connection when using HTTP/1.x for subsequent requests and can
    /// result in sub-optimal performance. If the response body is only a few MB
    /// and you are making multiple requests, then it is probably better to
    /// consume the response body and throw away the bytes rather than aborting
    /// and taking a hit on connection reuse. However, for operations such as
    /// file downloading, aborting in response to user input may be exactly what
    /// you want.
    ///
    /// If you drop a response with data still pending for the response body
    /// without calling this method, then the response will be aborted
    /// automatically and a warning emitted. If you intentionally _want_ abort
    /// behavior, then it is better to call this method explicitly so that no
    /// warning will be emitted.
    ///
    /// Note: The connection may not be immediately closed and the operation may
    /// be performed asynchronously.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use isahc::prelude::*;
    ///
    /// // Make a request and wait for the response headers.
    /// let response = isahc::get("https://httpbin.org")?;
    ///
    /// // Abort the request without reading the body at all.
    /// response.abort();
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    fn abort(self);
}

impl<T> ResponseExt<T> for Response<T> {
    fn effective_uri(&self) -> Option<&Uri> {
        self.extensions().get::<EffectiveUri>().map(|v| &v.0)
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        self.extensions().get::<LocalAddr>().map(|v| v.0)
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        self.extensions().get::<RemoteAddr>().map(|v| v.0)
    }

    #[cfg(feature = "cookies")]
    fn cookie_jar(&self) -> Option<&crate::cookies::CookieJar> {
        self.extensions().get()
    }

    fn metrics(&self) -> Option<&Metrics> {
        self.extensions().get()
    }

    fn abort(self) {
        if let Some(ctx) = self.extensions().get::<RequestContext>() {
            ctx.abort();
        } else {
            tracing::warn!("cannot abort responses not created by isahc");
        }
    }
}

/// Provides extension methods for consuming HTTP response streams.
pub trait ReadResponseExt<T: Read> {
    /// Copy the response body into a writer.
    ///
    /// Returns the number of bytes that were written.
    ///
    /// # Examples
    ///
    /// Copying the response into an in-memory buffer:
    ///
    /// ```no_run
    /// use isahc::prelude::*;
    ///
    /// let mut buf = vec![];
    /// isahc::get("https://example.org")?.copy_to(&mut buf)?;
    /// println!("Read {} bytes", buf.len());
    /// # Ok::<(), isahc::Error>(())
    /// ```
    fn copy_to<W: Write>(&mut self, writer: W) -> io::Result<u64>;

    /// Write the response body to a file.
    ///
    /// This method makes it convenient to download a file using a GET request
    /// and write it to a file synchronously in a single chain of calls.
    ///
    /// Returns the number of bytes that were written.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use isahc::prelude::*;
    ///
    /// isahc::get("https://httpbin.org/image/jpeg")?
    ///     .copy_to_file("myimage.jpg")?;
    /// # Ok::<(), isahc::Error>(())
    /// ```
    fn copy_to_file<P: AsRef<Path>>(&mut self, path: P) -> io::Result<u64> {
        File::create(path).and_then(|f| self.copy_to(f))
    }

    /// Read the response body as a string.
    ///
    /// The encoding used to decode the response body into a string depends on
    /// the response. If the body begins with a [Byte Order Mark
    /// (BOM)](https://en.wikipedia.org/wiki/Byte_order_mark), then UTF-8,
    /// UTF-16LE or UTF-16BE is used as indicated by the BOM. If no BOM is
    /// present, the encoding specified in the `charset` parameter of the
    /// `Content-Type` header is used if present. Otherwise UTF-8 is assumed.
    ///
    /// If the response body contains any malformed characters or characters not
    /// representable in UTF-8, the offending bytes will be replaced with
    /// `U+FFFD REPLACEMENT CHARACTER`, which looks like this: �.
    ///
    /// This method consumes the entire response body stream and can only be
    /// called once.
    ///
    /// # Availability
    ///
    /// This method is only available when the
    /// [`text-decoding`](index.html#text-decoding) feature is enabled, which it
    /// is by default.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use isahc::prelude::*;
    ///
    /// let text = isahc::get("https://example.org")?.text()?;
    /// println!("{}", text);
    /// # Ok::<(), isahc::Error>(())
    /// ```
    #[cfg(feature = "text-decoding")]
    fn text(&mut self) -> io::Result<String>;

    /// Deserialize the response body as JSON into a given type.
    ///
    /// # Availability
    ///
    /// This method is only available when the [`json`](index.html#json) feature
    /// is enabled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use isahc::prelude::*;
    /// use serde_json::Value;
    ///
    /// let json: Value = isahc::get("https://httpbin.org/json")?.json()?;
    /// println!("author: {}", json["slideshow"]["author"]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[cfg(feature = "json")]
    fn json<D>(&mut self) -> Result<D, serde_json::Error>
    where
        D: serde::de::DeserializeOwned;
}

impl<T: Read> ReadResponseExt<T> for Response<T> {
    fn copy_to<W: Write>(&mut self, mut writer: W) -> io::Result<u64> {
        io::copy(self.body_mut(), &mut writer)
    }

    #[cfg(feature = "text-decoding")]
    fn text(&mut self) -> io::Result<String> {
        crate::text::Decoder::for_response(&self).decode_reader(self.body_mut())
    }

    #[cfg(feature = "json")]
    fn json<D>(&mut self) -> Result<D, serde_json::Error>
    where
        D: serde::de::DeserializeOwned,
    {
        serde_json::from_reader(self.body_mut())
    }
}

/// Provides extension methods for consuming asynchronous HTTP response streams.
pub trait AsyncReadResponseExt<T: AsyncRead + Unpin> {
    /// Copy the response body into a writer asynchronously.
    ///
    /// Returns the number of bytes that were written.
    ///
    /// # Examples
    ///
    /// Copying the response into an in-memory buffer:
    ///
    /// ```no_run
    /// use isahc::prelude::*;
    ///
    /// # async fn run() -> Result<(), isahc::Error> {
    /// let mut buf = vec![];
    /// isahc::get_async("https://example.org").await?
    ///     .copy_to(&mut buf).await?;
    /// println!("Read {} bytes", buf.len());
    /// # Ok(()) }
    /// ```
    fn copy_to<'a, W>(&'a mut self, writer: W) -> CopyFuture<'a>
    where
        W: AsyncWrite + Unpin + 'a;

    /// Read the response body as a string asynchronously.
    ///
    /// This method consumes the entire response body stream and can only be
    /// called once.
    ///
    /// # Availability
    ///
    /// This method is only available when the
    /// [`text-decoding`](index.html#text-decoding) feature is enabled, which it
    /// is by default.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use isahc::prelude::*;
    ///
    /// # async fn run() -> Result<(), isahc::Error> {
    /// let text = isahc::get_async("https://example.org").await?
    ///     .text().await?;
    /// println!("{}", text);
    /// # Ok(()) }
    /// ```
    #[cfg(feature = "text-decoding")]
    fn text(&mut self) -> crate::text::TextFuture<'_, &mut T>;
}

impl<T: AsyncRead + Unpin> AsyncReadResponseExt<T> for Response<T> {
    fn copy_to<'a, W>(&'a mut self, writer: W) -> CopyFuture<'a>
    where
        W: AsyncWrite + Unpin + 'a,
    {
        CopyFuture(Box::pin(async move {
            futures_lite::io::copy(self.body_mut(), writer).await
        }))
    }

    #[cfg(feature = "text-decoding")]
    fn text(&mut self) -> crate::text::TextFuture<'_, &mut T> {
        crate::text::Decoder::for_response(&self).decode_reader_async(self.body_mut())
    }
}

/// A future which copies all the response body bytes into a sink.
#[allow(missing_debug_implementations)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct CopyFuture<'a>(Pin<Box<dyn Future<Output = io::Result<u64>> + 'a>>);

impl Future for CopyFuture<'_> {
    type Output = io::Result<u64>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.as_mut().poll(cx)
    }
}

pub(crate) struct LocalAddr(pub(crate) SocketAddr);

pub(crate) struct RemoteAddr(pub(crate) SocketAddr);
