//! Decode multiple [`Response`]s.

use k8s_openapi::{http::StatusCode, Response, ResponseError};

/// Provides an algorithm to parse multiple [`Response`]s from multiple chunks
/// of data represented as `&[u8]`.
#[derive(Debug)]
pub struct MultiResponseDecoder<T> {
    pending_data: Vec<u8>,
    responses_buffer: Vec<Result<T, ResponseError>>,
}

impl<T> MultiResponseDecoder<T>
where
    T: Response,
{
    /// Create a new [`MultiResponseDecoder`].
    pub fn new() -> Self {
        Self {
            pending_data: Vec::new(),
            responses_buffer: Vec::new(),
        }
    }

    /// Take the next chunk of data and spit out parsed `T`s.
    pub fn process_next_chunk(
        &mut self,
        chunk: &[u8],
    ) -> std::vec::Drain<'_, Result<T, ResponseError>> {
        self.pending_data.extend_from_slice(chunk);
        loop {
            match T::try_from_parts(StatusCode::OK, &self.pending_data) {
                Ok((response, consumed_bytes)) => {
                    debug_assert!(consumed_bytes > 0, "parser must've consumed some data");
                    self.pending_data.drain(..consumed_bytes);
                    self.responses_buffer.push(Ok(response));
                }
                Err(ResponseError::NeedMoreData) => break,
                Err(error) => {
                    self.responses_buffer.push(Err(error));
                    break;
                }
            };
        }
        self.responses_buffer.drain(..)
    }

    /// Complete the parsing.
    ///
    /// Call this when you're not expecting any more data chunks.
    /// Produces an error if there's unparsed data remaining.
    pub fn finish(self) -> Result<(), Vec<u8>> {
        let Self { pending_data, .. } = self;
        if pending_data.is_empty() {
            return Ok(());
        }
        Err(pending_data)
    }
}
