use std::{
    mem,
    pin::Pin,
    task::{self, Poll},
};

use futures::Stream as FutStream;
use parking_lot::Mutex;

use crate::{
    addr::Addr,
    context::Source,
    envelope::{Envelope, MessageKind},
    message::Message,
};

pub struct Stream<S>(Mutex<StreamState<S>>);

enum StreamState<S> {
    Active(Pin<Box<S>>),
    Closed,
}

impl<S> Stream<S> {
    pub fn new(stream: S) -> Self {
        Stream(Mutex::new(StreamState::Active(Box::pin(stream))))
    }

    pub fn set(&self, stream: S) {
        *self.0.lock() = StreamState::Active(Box::pin(stream));
    }

    pub fn replace(&self, stream: S) -> Option<S>
    where
        S: Unpin,
    {
        let new_state = StreamState::Active(Box::pin(stream));
        match mem::replace(&mut *self.0.lock(), new_state) {
            StreamState::Active(stream) => Some(*Pin::into_inner(stream)),
            StreamState::Closed => None,
        }
    }

    pub fn close(&self) -> bool {
        !matches!(
            mem::replace(&mut *self.0.lock(), StreamState::Closed),
            StreamState::Closed
        )
    }
}

impl<S> Source for Stream<S>
where
    S: FutStream,
    S::Item: Message,
{
    fn poll_recv(&self, cx: &mut task::Context<'_>) -> Poll<Option<Envelope>> {
        let mut state = self.0.lock();

        let stream = match &mut *state {
            StreamState::Active(stream) => stream,
            StreamState::Closed => return Poll::Pending, // TODO: `Poll::Ready(None)`?
        };

        match stream.as_mut().poll_next(cx) {
            Poll::Ready(Some(message)) => {
                let kind = MessageKind::Regular { sender: Addr::NULL };
                let envelope = Envelope::new(message, kind).upcast();
                Poll::Ready(Some(envelope))
            }
            Poll::Ready(None) => {
                *state = StreamState::Closed;
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}