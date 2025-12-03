use crate::{AccountProvider, Platform, Result};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait Scheduler: Send + Sync {
    async fn select(
        &self,
        platform: Platform,
        session_hash: Option<&str>,
    ) -> Result<Arc<dyn AccountProvider>>;

    fn accounts(&self, platform: Platform) -> Vec<Arc<dyn AccountProvider>>;

    fn all_accounts(&self) -> Vec<Arc<dyn AccountProvider>>;
}
