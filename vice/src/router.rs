//! the [`Router`] struct
use crate::{http::{into_response::IntoResponse, Request, Response}, util::service::NotFound};
use hyper::service::Service;
use std::{convert::Infallible, future::Future, pin::Pin, sync::Arc};

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
    /// assign new route
    pub fn route<R>(self, route: R) -> Router<RouteMatch<R, S>> {
        Router {
            inner: Arc::new(RouteMatch {
                inner: route,
                fallback: Arc::into_inner(self.inner)
                    .expect("`Router` should not be cloned in builder"),
            }),
        }
    }

    /// assign new route with early generic constraint check
    #[inline]
    pub fn route_checked<R>(self, route: R) -> Router<RouteMatch<R, S>>
    where
        R: Service<Request>,
        R::Response: IntoResponse,
        R::Error: IntoResponse,
        R::Future: Future<Output = Result<R::Response,R::Error>>,
    {
        self.route(route)
    }
}

impl<S> Service<Request> for Router<S>
where
    S: Service<Request> + Clone + Send + Sync + 'static,
    S::Response: IntoResponse + Send + 'static,
    S::Error: IntoResponse + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response,Self::Error>> + Send + 'static>>;

    fn call(&self, req: Request) -> Self::Future {
        let inner = Arc::clone(&self.inner);
        Box::pin(async move {
            Ok(inner.call(req).await.into_response())
        })
    }
}


#[derive(Clone)]
#[allow(dead_code)]
/// service that match request and delegate to either service
///
/// user typically does not interact with this directly, instead use [`route`] method
///
/// [`route`]: Router::route
pub struct RouteMatch<S,F> {
    inner: S,
    fallback: F,
}

