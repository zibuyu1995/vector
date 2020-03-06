use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::sink::Sink;
use pin_project::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

#[async_trait]
pub trait AsyncSink<Item> {
    type Error;

    async fn ready(&mut self) -> Result<(), Self::Error>;
    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error>;
    async fn flush(&mut self) -> Result<(), Self::Error>;
    async fn close(&mut self) -> Result<(), Self::Error>;
}

#[pin_project]
pub struct Driver<'a, Item, T: AsyncSink<Item> + Unpin + 'a> {
    #[pin]
    inner: Box<Pin<T>>,
    #[pin]
    state: State<'a, Result<(), <T as AsyncSink<Item>>::Error>>,
}

impl<'a, Item, T: AsyncSink<Item> + Unpin + 'a> Sink<Item> for Driver<'a, Item, T> {
    type Error = <T as AsyncSink<Item>>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.set_state_if_needed(State::PollReady(self.inner.ready()));
        self.poll_state(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        let this = self.project();
        this.inner.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.set_state_if_needed(State::PollFlush(self.inner.flush()));
        self.poll_state(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.set_state_if_needed(State::PollClose(self.inner.close()));
        self.poll_state(cx)
    }
}

impl<'a, Item, T: AsyncSink<Item> + Unpin + 'a> Driver<'a, Item, T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            state: State::Noop,
        }
    }
}

#[pin_project]
enum State<'a, T> {
    Noop,
    PollReady(#[pin] BoxFuture<'a, T>),
    PollFlush(#[pin] BoxFuture<'a, T>),
    PollClose(#[pin] BoxFuture<'a, T>),
}

// impl<'a, T> State<'a, T> {
//     fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) {
//         match Pin<&mut self> {
//             State::Noop => unreachable!(),
//             State::PollReady(mut fut) => fut.poll(cx),
//             State::PollFlush(mut fut) => fut.poll(cx),
//             State::PollClose(mut fut) => fut.poll(cx),
//         }
//     }
// }

impl<'a, Item, T: AsyncSink<Item> + Unpin + 'a> Driver<'a, Item, T> {
    fn poll_state(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), <T as AsyncSink<Item>>::Error>> {
        match self.state {
            State::Noop => unreachable!(),
            State::PollReady(ref mut fut) => fut.poll(cx),
            State::PollFlush(ref mut fut) => fut.poll(cx),
            State::PollClose(ref mut fut) => fut.poll(cx),
        }
    }

    fn set_state_if_needed(
        self: Pin<&mut Self>,
        new_state: State<'a, Result<(), <T as AsyncSink<Item>>::Error>>,
    ) {
        if let State::Noop = self.state {
            self.state = new_state;
        }
    }
}
