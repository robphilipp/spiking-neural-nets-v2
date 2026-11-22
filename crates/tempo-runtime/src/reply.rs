//! A one-shot reply: the answer to a control-plane request. Wait for it with [`Reply::wait`] from ordinary code,
//! or `.await` it from async code (it implements [`Future`]), so a web or desktop layer can use it without the
//! runtime depending on an async runtime.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

/// The request was dropped without an answer, e.g. because the runtime was dropped.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the request was dropped without a reply")
    }
}

impl std::error::Error for Cancelled {}

#[derive(Debug)]
struct State<T> {
    value: Option<T>,
    closed: bool,
    waker: Option<Waker>,
}

#[derive(Debug)]
struct Inner<T> {
    state: Mutex<State<T>>,
    ready: Condvar,
}

impl<T> Inner<T> {
    fn lock(&self) -> MutexGuard<'_, State<T>> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// The receiving end of a request's answer.
#[derive(Debug)]
pub struct Reply<T> {
    inner: Arc<Inner<T>>,
}

/// The sending end; answering consumes it. Dropping it unanswered cancels the reply.
#[derive(Debug)]
pub(crate) struct Responder<T> {
    inner: Arc<Inner<T>>,
}

/// A new reply and its responder.
pub(crate) fn reply<T>() -> (Responder<T>, Reply<T>) {
    let inner = Arc::new(Inner {
        state: Mutex::new(State {
            value: None,
            closed: false,
            waker: None,
        }),
        ready: Condvar::new(),
    });
    (
        Responder {
            inner: Arc::clone(&inner),
        },
        Reply { inner },
    )
}

impl<T> Responder<T> {
    pub(crate) fn send(self, value: T) {
        let mut state = self.inner.lock();
        state.value = Some(value);
        // `self` drops after this, which marks the reply closed and wakes the receiver.
    }
}

impl<T> Drop for Responder<T> {
    fn drop(&mut self) {
        let waker = {
            let mut state = self.inner.lock();
            state.closed = true;
            state.waker.take()
        };
        self.inner.ready.notify_all();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl<T> Reply<T> {
    /// Blocks until the answer arrives.
    ///
    /// # Errors
    /// [`Cancelled`] if the request was dropped unanswered.
    pub fn wait(self) -> Result<T, Cancelled> {
        let mut state = self.inner.lock();
        loop {
            if let Some(value) = state.value.take() {
                return Ok(value);
            }
            if state.closed {
                return Err(Cancelled);
            }
            state = self.inner.ready.wait(state).unwrap_or_else(|p| p.into_inner());
        }
    }

    /// Blocks for at most `timeout`. Returns `None` if no answer arrived in time.
    pub fn wait_timeout(&self, timeout: Duration) -> Option<Result<T, Cancelled>> {
        let deadline = std::time::Instant::now() + timeout;
        let mut state = self.inner.lock();
        loop {
            if let Some(value) = state.value.take() {
                return Some(Ok(value));
            }
            if state.closed {
                return Some(Err(Cancelled));
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            state = self
                .inner
                .ready
                .wait_timeout(state, remaining)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }

    /// The answer, if it has arrived.
    pub fn try_take(&self) -> Option<Result<T, Cancelled>> {
        let mut state = self.inner.lock();
        match state.value.take() {
            Some(value) => Some(Ok(value)),
            None if state.closed => Some(Err(Cancelled)),
            None => None,
        }
    }
}

impl<T> Future for Reply<T> {
    type Output = Result<T, Cancelled>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.inner.lock();
        if let Some(value) = state.value.take() {
            return Poll::Ready(Ok(value));
        }
        if state.closed {
            return Poll::Ready(Err(Cancelled));
        }
        state.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answer_from_another_thread() {
        let (tx, rx) = reply();
        std::thread::spawn(move || tx.send(42));
        assert_eq!(rx.wait(), Ok(42));
    }

    #[test]
    fn dropping_the_responder_cancels() {
        let (tx, rx) = reply::<u32>();
        drop(tx);
        assert_eq!(rx.wait(), Err(Cancelled));
    }

    #[test]
    fn timeout_without_answer() {
        let (_tx, rx) = reply::<u32>();
        assert!(rx.wait_timeout(Duration::from_millis(5)).is_none());
    }

    #[test]
    fn works_as_a_future() {
        // A minimal executor: poll until ready, parking between polls.
        struct ThreadWaker(std::thread::Thread);
        impl std::task::Wake for ThreadWaker {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
        }
        let (tx, mut rx) = reply();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(5));
            tx.send("done");
        });
        let waker = Waker::from(Arc::new(ThreadWaker(std::thread::current())));
        let mut cx = Context::from_waker(&waker);
        let result = loop {
            match Pin::new(&mut rx).poll(&mut cx) {
                Poll::Ready(v) => break v,
                Poll::Pending => std::thread::park(),
            }
        };
        assert_eq!(result, Ok("done"));
    }
}
