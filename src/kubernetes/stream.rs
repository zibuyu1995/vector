//! Work with HTTP bodies as streams of Kubernetes resources.

use super::multi_response_decoder::MultiResponseDecoder;
use async_stream::try_stream;
use bytes05::Buf;
use futures::pin_mut;
use futures::stream::Stream;
use http_body::Body;
use k8s_openapi::{Response, ResponseError};
use snafu::{ResultExt, Snafu};

/// Converts the HTTP response [`Body`] to a stream of parsed Kubernetes
/// [`Response`]s.
pub fn body<B, T>(body: B) -> impl Stream<Item = Result<T, Error<<B as Body>::Error>>>
where
    T: Response + Unpin + 'static,
    B: Body,
    <B as Body>::Error: std::error::Error + 'static + Unpin,
{
    try_stream! {
        let mut decoder: MultiResponseDecoder<T> = MultiResponseDecoder::new();

        debug!(message = "streaming the HTTP body");

        pin_mut!(body);
        while let Some(buf) = body.data().await {
            let mut buf = buf.context(Reading)?;
            let chunk = buf.to_bytes();
            let responses = decoder.process_next_chunk(chunk.as_ref());
            for response in responses {
                let response = response.context(Parsing)?;
                yield response;
            }
        }
        decoder.finish().map_err(|data| Error::UnparsedDataUponCompletion { data })?;
    }
}

/// Errors that can occur in the stream.
#[derive(Debug, Snafu)]
pub enum Error<ReadError>
where
    ReadError: std::error::Error + 'static,
{
    /// An error occured while reading the response body.
    #[snafu(display("reading the data chunk failed"))]
    Reading {
        /// The error we got while reading.
        source: ReadError,
    },

    /// An error occured while parsing the response body.
    #[snafu(display("data parsing failed"))]
    Parsing {
        /// Response parsing error.
        source: ResponseError,
    },

    /// An incomplete response remains in the buffer, but we don't expect
    /// any more data.
    #[snafu(display("unparsed data remaining upon completion"))]
    UnparsedDataUponCompletion {
        /// The unparsed data.
        data: Vec<u8>,
    },
}
