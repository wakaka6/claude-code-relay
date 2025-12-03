use crate::{AccountProvider, Result};
use async_trait::async_trait;
use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;

pub type BoxStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>;

#[async_trait]
pub trait Relay: Send + Sync {
    type Request: Send;
    type Response: Send;

    async fn relay(
        &self,
        account: &dyn AccountProvider,
        request: Self::Request,
    ) -> Result<Self::Response>;

    async fn relay_stream(
        &self,
        account: &dyn AccountProvider,
        request: Self::Request,
    ) -> Result<BoxStream<Result<Bytes>>>;
}
