use crate::sinks::RouterSink;
use crate::Event;
use futures::sync::mpsc;
use futures::{future, Async, AsyncSink, Poll, Sink, StartSend, Stream};
use std::collections::HashMap;

pub struct Multiplex {
    sinks: HashMap<String, MultiplexedSink>,
}

pub type MultiplexedSink = Box<dyn Sink<SinkItem = Event, SinkError = ()> + 'static + Send>;

impl Multiplex {
    pub fn new(sinks: HashMap<String, MultiplexedSink>) -> Self {
        Self { sinks }
    }

    fn remove(&mut self, name: &str) {
        let mut removed = self.sinks.remove(name).expect("Didn't find output in multiplexer");
        tokio::spawn(future::poll_fn(move || removed.close()));
    }
}

impl Sink for Multiplex {
    type SinkItem = (String, Event);
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if self.sinks.is_empty() {
            return Ok(AsyncSink::Ready);
        }

        if let Some(sink) = self.sinks.get_mut(&item.0) {
            match sink.start_send(item.1) {
                Ok(AsyncSink::NotReady(ret_item)) => return Ok(AsyncSink::NotReady((item.0, ret_item))),
                Ok(AsyncSink::Ready) => (),
                Err(()) => return Err(()),
            }
        }

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        let mut all_complete = true;

        for (_, sink) in &mut self.sinks {
            match sink.poll_complete() {
                Ok(Async::Ready(())) => {}
                Ok(Async::NotReady) => {
                    all_complete = false;
                }
                Err(()) => return Err(()),
            }
        }

        if all_complete {
            Ok(Async::Ready(()))
        } else {
            Ok(Async::NotReady)
        }
    }
}
