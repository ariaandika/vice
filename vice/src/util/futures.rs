//! future utility types
use std::{marker::PhantomData, task::{ready, Poll}};

use super::Either;

/// extension trait for `Future` trait
pub trait FutureExt: Future {
    /// map the future output
    fn map<M,R>(self, mapper: M) -> Map<Self,M>
    where
        M: FnOnce(Self::Output) -> R,
        Self: Sized,
    {
        Map { inner: self, mapper: Some(mapper)  }
    }

    /// map the future output into `Result<T,Infallible>`
    fn map_infallible(self) -> MapInfallible<Self>
    where
        Self: Sized
    {
        MapInfallible { inner: self }
    }

    /// map the future output into `Result<T,Infallible>`
    fn and_then_or<M,L,R>(self, mapper: M) -> AndThenOr<Self,M,L>
    where
        M: FnOnce(Self::Output) -> Result<L,R>,
        L: Future<Output = R>,
        Self: Sized,
    {
        AndThenOr::First { f: self, mapper: Some(mapper) }
    }
}

impl<F> FutureExt for F where F: Future { }

// ---

pin_project_lite::pin_project! {
    /// map the output of a future
    pub struct Map<F,M> {
        #[pin]
        inner: F,
        mapper: Option<M>,
    }
}

impl<F,M,R> Future for Map<F,M>
where
    F: Future,
    M: FnOnce(F::Output) -> R,
{
    type Output = R;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let me = self.project();
        Poll::Ready((me.mapper.take().expect("poll after complete"))(ready!(me.inner.poll(cx))))
    }
}

// ---

pin_project_lite::pin_project! {
    pub struct MapInfallible<F> {
        #[pin]
        inner: F
    }
}

impl<F> Future for MapInfallible<F>
where
    F: Future,
{
    type Output = Result<F::Output, std::convert::Infallible>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(Ok(ready!(self.project().inner.poll(cx))))
    }
}

// ---

pin_project_lite::pin_project! {
    #[project = AndThenOrProj]
    pub enum AndThenOr<F,M,L> {
        First { #[pin] f: F, mapper: Option<M> },
        Second { #[pin] f: L },
    }
}

impl<F,M,L,R> Future for AndThenOr<F,M,L>
where
    F: Future,
    M: FnOnce(F::Output) -> Result<L,R>,
    L: Future<Output = R>,
{
    type Output = R;

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        loop {
            match self.as_mut().project() {
                AndThenOrProj::First { f, mapper } => {
                    let ok = ready!(f.poll(cx));
                    match (mapper.take().expect("poll after complete"))(ok) {
                        Ok(fut2) => {
                            self.set(AndThenOr::Second { f: fut2 });
                        },
                        Err(r) => return Poll::Ready(r),
                    }
                },
                AndThenOrProj::Second { f } => return f.poll(cx),
            }
        }
    }
}

pin_project_lite::pin_project! {
    #[project = EitherProj]
    pub enum EitherFuture<L,R> {
        Left { #[pin] left: L },
        Right { #[pin] right: R },
    }
}

impl<L,R> Future for EitherFuture<L,R>
where
    L: Future,
    R: Future,
{
    type Output = Either<L::Output,R::Output>;

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project() {
            EitherProj::Left { left } => Poll::Ready(Either::Left(ready!(left.poll(cx)))),
            EitherProj::Right { right } => Poll::Ready(Either::Right(ready!(right.poll(cx)))),
        }
    }
}

pin_project_lite::pin_project! {
    #[project = EitherIntoProj]
    pub enum EitherInto<L,R,O> {
        Left { #[pin] left: L, _p: PhantomData<O> },
        Right { #[pin] right: R },
    }
}

impl<L,R,O> Future for EitherInto<L,R,O>
where
    L: Future,
    R: Future,
    L::Output: Into<O>,
    R::Output: Into<O>,
{
    type Output = O;

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project() {
            EitherIntoProj::Left { left, .. } => Poll::Ready(ready!(left.poll(cx)).into()),
            EitherIntoProj::Right { right } => Poll::Ready(ready!(right.poll(cx)).into()),
        }
    }
}

