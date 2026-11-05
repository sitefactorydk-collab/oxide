//! Turns an SSE response into a [`ChatStream`] with a provider-specific decoder.

use std::collections::VecDeque;

use futures_util::StreamExt;
use futures_util::stream;

use crate::http::describe;
use crate::sse::{SseEvent, SseParser};
use crate::{ChatEvent, ChatStream, LlmError};

pub(crate) type Out = VecDeque<Result<ChatEvent, LlmError>>;

/// Decodes one provider's SSE events into chat events.
pub(crate) trait Decoder: Send + 'static {
    /// Handles one event. Returns `true` when the reply is complete (or failed) and no more
    /// events should be read.
    fn event(&mut self, event: SseEvent, out: &mut Out) -> bool;

    /// The connection ended without the decoder finishing.
    fn eof(&mut self, out: &mut Out);
}

struct Driver<D> {
    response: Option<reqwest::Response>,
    parser: SseParser,
    decoder: D,
    out: Out,
}

/// Streams `response` through `decoder`. The stream ends after the decoder finishes, after
/// the first error, or when the connection closes.
pub(crate) fn sse_stream<D: Decoder>(response: reqwest::Response, decoder: D) -> ChatStream {
    let driver = Driver {
        response: Some(response),
        parser: SseParser::new(),
        decoder,
        out: VecDeque::new(),
    };
    stream::unfold(driver, |mut d| async move {
        loop {
            if let Some(item) = d.out.pop_front() {
                if item.is_err() {
                    d.out.clear();
                    d.response = None;
                }
                return Some((item, d));
            }
            let response = d.response.as_mut()?;
            match response.chunk().await {
                Ok(Some(bytes)) => {
                    for ev in d.parser.push(&bytes) {
                        if d.decoder.event(ev, &mut d.out) {
                            d.response = None;
                            break;
                        }
                    }
                }
                Ok(None) => {
                    d.response = None;
                    let finished = d
                        .parser
                        .finish()
                        .is_some_and(|ev| d.decoder.event(ev, &mut d.out));
                    if !finished {
                        d.decoder.eof(&mut d.out);
                    }
                }
                Err(e) => {
                    d.response = None;
                    d.out.push_back(Err(LlmError::Stream(format!(
                        "the connection broke off: {}",
                        describe(&e)
                    ))));
                }
            }
        }
    })
    .boxed()
}
