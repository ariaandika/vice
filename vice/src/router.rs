//! the [`Router`] struct
use crate::{
    http::{Request, Response},
    util::{futures::EitherInto, service::NotFound, Either},
};
use hyper::service::Service;
use std::{convert::Infallible, sync::Arc};

pub mod handler;

/// route builder
///
/// # Service
///
/// this implements [`Service`] that can be used in [`listen`]
///
/// [`listen`]: crate::listen
///
/// # Example
///
/// ```
/// fn main() -> std::io::Result<()> {
///     let route = Router::new();
///     vice::listen("0.0.0.0:3000", route)
/// }
/// ```
#[derive(Clone)]
pub struct Router<S> {
    inner: Arc<S>
}

impl Router<NotFound> {
    /// create new `Router`
    pub fn new() -> Router<NotFound> {
        Router { inner: Arc::new(NotFound) }
    }
}

impl<S> Router<S> {
    /// create new `Router` with custom fallback
    pub fn new_with_fallback(fallback: S) -> Router<S> {
        Router { inner: Arc::new(fallback) }
    }

    /// assign new route
    pub fn route<R>(self, matches: impl Into<RequestMatcher>, route: R) -> Router<Branch<R, S>> {
        Router {
            inner: Arc::new(Branch {
                matcher: matches.into(),
                inner: route,
                fallback: Arc::into_inner(self.inner)
                    .expect("`Router` should not be cloned in builder"),
            }),
        }
    }

    /// assign new route with early generic constraint check
    #[inline]
    pub fn route_checked<R>(self, path: &'static str, route: R) -> Router<Branch<R, S>>
    where
        R: Service<Request, Response = Response, Error = Infallible>,
    {
        self.route(path, route)
    }
}

impl<S> Service<Request> for Router<S>
where
    S: Service<Request, Response = Response, Error = Infallible> + Clone + Send + Sync + 'static,
{
    type Response = Response;
    type Error = Infallible;
    type Future = S::Future;

    fn call(&self, req: Request) -> Self::Future {
        Arc::clone(&self.inner).call(req)
    }
}

impl Default for Router<NotFound> {
    fn default() -> Self {
        Self::new()
    }
}


#[derive(Clone)]
#[allow(dead_code)]
/// service that match request and delegate to either service
///
/// user typically does not interact with this directly, instead use [`route`] method
///
/// [`route`]: Router::route
pub struct Branch<S,F> {
    matcher: RequestMatcher,
    inner: S,
    fallback: F,
}

impl<S,F> Service<Request> for Branch<S,F>
where
    S: Service<Request, Response = Response, Error = Infallible>,
    F: Service<Request, Response = Response, Error = Infallible>,
{
    type Response = Response;
    type Error = Infallible;
    type Future = EitherInto<S::Future,F::Future,Result<Response,Infallible>>;

    fn call(&self, req: Request) -> Self::Future {
        match self.matcher == req {
            true => Either::Left(self.inner.call(req)).await_into(),
            false => Either::Right(self.fallback.call(req)).await_into(),
        }
    }
}

#[derive(Clone)]
pub struct RequestMatcher {
    path: Option<&'static str>,
    method: Option<http::Method>,
}

impl<T> PartialEq<Request<T>> for RequestMatcher {
    fn eq(&self, other: &Request<T>) -> bool {
        if matches!(self.path,Some(path) if path != other.uri().path()) {
            return false;
        }
        if matches!(&self.method,Some(method) if method != other.method()) {
            return false;
        }
        true
    }
}

impl From<&'static str> for RequestMatcher {
    fn from(value: &'static str) -> Self {
        Self { path: Some(value), method: None }
    }
}

