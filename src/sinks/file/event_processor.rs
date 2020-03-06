use crate::Event;
use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::ready;
use futures::Sink;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A push-based event handling API.
#[async_trait]
pub trait EventProcessor: Send + Sync + 'static {
    type Error;

    async fn process(&mut self, input: Event) -> Result<(), Self::Error>;
}

pub struct EventProcessorAsSink<'a, T: EventProcessor> {
    inner: T,
    state: Option<BoxFuture<'a, Result<(), <T as EventProcessor>::Error>>>,
}

impl<'a, T: EventProcessor> EventProcessorAsSink<'a, T> {
    pub fn new(inner: T) -> Self {
        Self { inner, state: None }
    }

    pub fn new_box(inner: T) -> crate::sinks::RouterSink
    where
        T: EventProcessor<Error = ()>,
    {
        Box::new(futures::compat::CompatSink::new(Box::pin(Self::new(inner))))
    }

    fn poll_consume_state(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), <T as EventProcessor>::Error>> {
        if self.state.is_none() {
            return Poll::Ready(Ok(()));
        }

        let val = ready!(self.state.unwrap().as_mut().poll(cx));
        self.state = None;

        Poll::Ready(val)
    }
}

impl<'a, T: EventProcessor> Sink<Event> for EventProcessorAsSink<'a, T> {
    type Error = <T as EventProcessor>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_consume_state(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Event) -> Result<(), Self::Error> {
        debug_assert!(self.state.is_none());
        let new_state = Box::pin(self.inner.process(item));
        self.state = Some(new_state);
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_consume_state(cx)
    }
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_consume_state(cx)
    }
}
