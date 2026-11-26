//! Typed observation of one erased blocking task; dropping it still detaches.
use std::{
    any::Any,
    error::Error,
    fmt,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task::{JoinError, JoinHandle};

pub(crate) type Erased = Box<dyn Any + Send>;

#[derive(Debug)]
pub(crate) enum Failure {
    Join(JoinError),
    ResultType,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Join(error) => error.fmt(f),
            Self::ResultType => f.write_str("Blocking worker returned an incompatible result"),
        }
    }
}
impl From<Failure> for std::io::Error {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Join(error) => error.into(),
            Failure::ResultType => std::io::Error::other(Failure::ResultType),
        }
    }
}
impl Error for Failure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Join(error) => Some(error),
            Self::ResultType => None,
        }
    }
}
#[cfg(test)]
impl Failure {
    pub(crate) fn is_panic(&self) -> bool {
        matches!(self, Self::Join(error) if error.is_panic())
    }
}

pub(crate) struct Blocking<R> {
    task: JoinHandle<Erased>,
    result: PhantomData<fn() -> R>,
}
impl<R: Send + 'static> Blocking<R> {
    pub(super) fn new(task: JoinHandle<Erased>) -> Self {
        Self {
            task,
            result: PhantomData,
        }
    }
    #[cfg(test)]
    pub(crate) fn abort(&self) {
        self.task.abort();
    }
}
impl<R: Send + 'static> Future for Blocking<R> {
    type Output = Result<R, Failure>;
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.get_mut().task).poll(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(Failure::Join(error))),
            Poll::Ready(Ok(result)) => Poll::Ready(
                result
                    .downcast::<R>()
                    .map(|result| *result)
                    .map_err(|_| Failure::ResultType),
            ),
        }
    }
}
