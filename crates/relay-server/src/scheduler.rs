use crate::db::{self, DbPool};
use parking_lot::RwLock;
use relay_core::{generate_session_hash, AccountProvider, Platform, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

pub struct AccountCooldown {
    until: Instant,
    #[allow(dead_code)] // Reserved for future logging/debugging
    reason: String,
}

pub struct AccountUsage {
    last_used: Instant,
    request_count: u64,
}

pub struct UnifiedScheduler {
    accounts: Vec<Arc<dyn AccountProvider>>,
    db_pool: DbPool,
    cooldowns: RwLock<HashMap<String, AccountCooldown>>,
    usage: RwLock<HashMap<String, AccountUsage>>,
    sticky_ttl: Duration,
    renewal_threshold: Duration,
    unavailable_cooldown: Duration,
}

impl UnifiedScheduler {
    pub fn new(
        accounts: Vec<Arc<dyn AccountProvider>>,
        sticky_ttl_secs: u64,
        renewal_threshold_secs: u64,
        unavailable_cooldown_secs: u64,
        db_pool: DbPool,
    ) -> Self {
        Self {
            accounts,
            db_pool,
            cooldowns: RwLock::new(HashMap::new()),
            usage: RwLock::new(HashMap::new()),
            sticky_ttl: Duration::from_secs(sticky_ttl_secs),
            renewal_threshold: Duration::from_secs(renewal_threshold_secs),
            unavailable_cooldown: Duration::from_secs(unavailable_cooldown_secs),
        }
    }

    pub fn mark_account_rate_limited(&self, account_id: &str, retry_after_secs: u64) {
        let mut cooldowns = self.cooldowns.write();
        let until = Instant::now() + Duration::from_secs(retry_after_secs);
        cooldowns.insert(
            account_id.to_string(),
            AccountCooldown {
                until,
                reason: "rate_limited".to_string(),
            },
        );
        info!(
            account_id = account_id,
            retry_after_secs = retry_after_secs,
            "Account marked as rate limited"
        );
    }

    pub fn mark_account_overloaded(&self, account_id: &str, minutes: u64) {
        let mut cooldowns = self.cooldowns.write();
        let until = Instant::now() + Duration::from_secs(minutes * 60);
        cooldowns.insert(
            account_id.to_string(),
            AccountCooldown {
                until,
                reason: "overloaded".to_string(),
            },
        );
        info!(
            account_id = account_id,
            minutes = minutes,
            "Account marked as overloaded"
        );
    }

    pub fn mark_account_unavailable(&self, account_id: &str, reason: &str) {
        let mut cooldowns = self.cooldowns.write();
        let until = Instant::now() + self.unavailable_cooldown;
        cooldowns.insert(
            account_id.to_string(),
            AccountCooldown {
                until,
                reason: reason.to_string(),
            },
        );
        warn!(
            account_id = account_id,
            reason = reason,
            cooldown_seconds = self.unavailable_cooldown.as_secs(),
            "Account marked as unavailable"
        );
    }

    fn is_account_in_cooldown(&self, account_id: &str) -> bool {
        let cooldowns = self.cooldowns.read();
        if let Some(cooldown) = cooldowns.get(account_id) {
            if Instant::now() < cooldown.until {
                return true;
            }
        }
        false
    }

    fn record_account_used(&self, account_id: &str) {
        let mut usage = self.usage.write();
        let entry = usage.entry(account_id.to_string()).or_insert(AccountUsage {
            last_used: Instant::now(),
            request_count: 0,
        });
        entry.last_used = Instant::now();
        entry.request_count += 1;
    }

    fn get_last_used(&self, account_id: &str) -> Option<Instant> {
        let usage = self.usage.read();
        usage.get(account_id).map(|u| u.last_used)
    }

    pub async fn select_account(
        &self,
        platform: Platform,
        request_body: &serde_json::Value,
    ) -> Result<Arc<dyn AccountProvider>> {
        self.select_account_excluding(platform, request_body, &HashSet::new())
            .await
    }

    pub async fn select_account_excluding(
        &self,
        platform: Platform,
        request_body: &serde_json::Value,
        excluded: &HashSet<String>,
    ) -> Result<Arc<dyn AccountProvider>> {
        let session_hash = generate_session_hash(request_body);

        if let Some(ref hash) = session_hash {
            if let Some(account) = self.get_sticky_account(hash, platform, excluded).await {
                debug!(session_hash = %hash, account_id = account.id(), "Using sticky session account");
                self.record_account_used(account.id());
                return Ok(account);
            }
        }

        let account = self.select_available_account(platform, excluded)?;

        if let Some(hash) = session_hash {
            self.set_sticky_session(&hash, account.id()).await;
            debug!(session_hash = %hash, account_id = account.id(), "Created new sticky session");
        }

        info!(
            account_id = account.id(),
            account_name = account.name(),
            priority = account.priority(),
            platform = ?platform,
            "Selected account for request"
        );

        self.record_account_used(account.id());
        Ok(account)
    }

    async fn get_sticky_account(
        &self,
        session_hash: &str,
        platform: Platform,
        excluded: &HashSet<String>,
    ) -> Option<Arc<dyn AccountProvider>> {
        // Query database for sticky session
        let session = match db::get_sticky_session(&self.db_pool, session_hash).await {
            Ok(Some(s)) => s,
            Ok(None) => return None,
            Err(e) => {
                warn!(error = %e, session_hash = %session_hash, "Failed to get sticky session");
                return None;
            }
        };

        let (account_id, remaining_secs) = session;

        // Check if account is excluded or in cooldown
        if excluded.contains(&account_id) {
            return None;
        }
        if self.is_account_in_cooldown(&account_id) {
            return None;
        }

        // Find the account
        let account = self.accounts.iter().find(|a| {
            a.id() == account_id && a.platform() == platform && a.is_available()
        })?;

        // Smart renewal: only renew if remaining time < threshold
        if remaining_secs < self.renewal_threshold.as_secs() as i64 {
            let ttl = self.sticky_ttl.as_secs() as i64;
            if let Err(e) =
                db::upsert_sticky_session(&self.db_pool, session_hash, &account_id, ttl).await
            {
                warn!(error = %e, session_hash = %session_hash, "Failed to renew sticky session");
            } else {
                debug!(session_hash = %session_hash, "Renewed sticky session");
            }
        }

        Some(account.clone())
    }

    async fn set_sticky_session(&self, session_hash: &str, account_id: &str) {
        let ttl = self.sticky_ttl.as_secs() as i64;
        if let Err(e) =
            db::upsert_sticky_session(&self.db_pool, session_hash, account_id, ttl).await
        {
            warn!(error = %e, session_hash = %session_hash, "Failed to set sticky session");
        }
    }

    fn select_available_account(
        &self,
        platform: Platform,
        excluded: &HashSet<String>,
    ) -> Result<Arc<dyn AccountProvider>> {
        let mut available: Vec<_> = self
            .accounts
            .iter()
            .filter(|a| {
                a.platform() == platform
                    && a.is_available()
                    && !excluded.contains(a.id())
                    && !self.is_account_in_cooldown(a.id())
            })
            .cloned()
            .collect();

        if available.is_empty() {
            warn!(platform = ?platform, "No available accounts for platform");
            return Err(relay_core::RelayError::NoAccount(platform));
        }

        available.sort_by(|a, b| {
            let priority_cmp = b.priority().cmp(&a.priority());
            if priority_cmp != std::cmp::Ordering::Equal {
                return priority_cmp;
            }

            let a_last_used = self.get_last_used(a.id());
            let b_last_used = self.get_last_used(b.id());

            match (a_last_used, b_last_used) {
                (Some(a_time), Some(b_time)) => a_time.cmp(&b_time),
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });

        Ok(available.remove(0))
    }

    pub fn cleanup_expired_cooldowns(&self) {
        let now = Instant::now();
        let mut cooldowns = self.cooldowns.write();
        let before = cooldowns.len();
        cooldowns.retain(|_, cooldown| now < cooldown.until);
        let removed = before - cooldowns.len();
        if removed > 0 {
            debug!(removed = removed, "Cleaned up expired account cooldowns");
        }
    }

    #[allow(dead_code)] // Reserved for admin API
    pub fn get_accounts_by_platform(&self, platform: Platform) -> Vec<Arc<dyn AccountProvider>> {
        self.accounts
            .iter()
            .filter(|a| a.platform() == platform)
            .cloned()
            .collect()
    }

    #[allow(dead_code)] // Reserved for admin API
    pub fn get_all_accounts(&self) -> &[Arc<dyn AccountProvider>] {
        &self.accounts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use relay_core::{Credentials, ProxyConfig};
    use std::sync::atomic::{AtomicBool, Ordering};

    struct MockAccount {
        id: String,
        name: String,
        platform: Platform,
        priority: u32,
        available: AtomicBool,
    }

    impl MockAccount {
        fn new(id: &str, platform: Platform, priority: u32) -> Self {
            Self {
                id: id.to_string(),
                name: format!("Mock {}", id),
                platform,
                priority,
                available: AtomicBool::new(true),
            }
        }
    }

    #[async_trait]
    impl AccountProvider for MockAccount {
        fn id(&self) -> &str {
            &self.id
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn platform(&self) -> Platform {
            self.platform
        }

        fn priority(&self) -> u32 {
            self.priority
        }

        fn is_available(&self) -> bool {
            self.available.load(Ordering::SeqCst)
        }

        async fn get_credentials(&self) -> relay_core::Result<Credentials> {
            Ok(Credentials::ApiKey("test-key".to_string()))
        }

        fn proxy_config(&self) -> Option<&ProxyConfig> {
            None
        }

        fn mark_unavailable(&self, _duration: Duration, _reason: &str) {
            self.available.store(false, Ordering::SeqCst);
        }

        fn mark_available(&self) {
            self.available.store(true, Ordering::SeqCst);
        }
    }

    async fn setup_test_db() -> DbPool {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let path_str = path.to_str().unwrap().to_string();
        std::mem::forget(dir);
        db::init_database(&path_str).await.unwrap()
    }

    async fn setup_scheduler() -> (UnifiedScheduler, DbPool) {
        let pool = setup_test_db().await;
        let accounts: Vec<Arc<dyn AccountProvider>> = vec![
            Arc::new(MockAccount::new("acc1", Platform::Claude, 100)),
            Arc::new(MockAccount::new("acc2", Platform::Claude, 50)),
        ];
        let scheduler = UnifiedScheduler::new(accounts, 3600, 300, 3600, pool.clone());
        (scheduler, pool)
    }

    // ========================================================================
    // Existing tests (adapted)
    // ========================================================================

    #[tokio::test]
    async fn test_scheduler_creation_with_custom_cooldown() {
        let pool = setup_test_db().await;
        let accounts: Vec<Arc<dyn AccountProvider>> =
            vec![Arc::new(MockAccount::new("test-1", Platform::Claude, 100))];

        let scheduler = UnifiedScheduler::new(accounts, 3600, 300, 1800, pool);

        assert_eq!(scheduler.sticky_ttl, Duration::from_secs(3600));
        assert_eq!(scheduler.renewal_threshold, Duration::from_secs(300));
        assert_eq!(scheduler.unavailable_cooldown, Duration::from_secs(1800));
    }

    #[tokio::test]
    async fn test_mark_account_unavailable_uses_configured_cooldown() {
        let pool = setup_test_db().await;
        let accounts: Vec<Arc<dyn AccountProvider>> =
            vec![Arc::new(MockAccount::new("test-1", Platform::Claude, 100))];

        let scheduler = UnifiedScheduler::new(accounts, 3600, 300, 5, pool);

        scheduler.mark_account_unavailable("test-1", "test_reason");

        assert!(scheduler.is_account_in_cooldown("test-1"));

        let cooldowns = scheduler.cooldowns.read();
        let cooldown = cooldowns.get("test-1").unwrap();
        let remaining = cooldown.until.duration_since(Instant::now());
        assert!(remaining <= Duration::from_secs(5));
        assert!(remaining >= Duration::from_secs(4));
    }

    #[tokio::test]
    async fn test_mark_account_rate_limited() {
        let pool = setup_test_db().await;
        let accounts: Vec<Arc<dyn AccountProvider>> =
            vec![Arc::new(MockAccount::new("test-1", Platform::Claude, 100))];

        let scheduler = UnifiedScheduler::new(accounts, 3600, 300, 3600, pool);

        scheduler.mark_account_rate_limited("test-1", 60);

        assert!(scheduler.is_account_in_cooldown("test-1"));

        let cooldowns = scheduler.cooldowns.read();
        let cooldown = cooldowns.get("test-1").unwrap();
        assert_eq!(cooldown.reason, "rate_limited");
    }

    #[tokio::test]
    async fn test_mark_account_overloaded() {
        let pool = setup_test_db().await;
        let accounts: Vec<Arc<dyn AccountProvider>> =
            vec![Arc::new(MockAccount::new("test-1", Platform::Claude, 100))];

        let scheduler = UnifiedScheduler::new(accounts, 3600, 300, 3600, pool);

        scheduler.mark_account_overloaded("test-1", 5);

        assert!(scheduler.is_account_in_cooldown("test-1"));

        let cooldowns = scheduler.cooldowns.read();
        let cooldown = cooldowns.get("test-1").unwrap();
        assert_eq!(cooldown.reason, "overloaded");
    }

    #[tokio::test]
    async fn test_cooldown_cleanup() {
        let pool = setup_test_db().await;
        let accounts: Vec<Arc<dyn AccountProvider>> =
            vec![Arc::new(MockAccount::new("test-1", Platform::Claude, 100))];

        let scheduler = UnifiedScheduler::new(accounts, 3600, 300, 0, pool);

        scheduler.mark_account_unavailable("test-1", "test_reason");

        std::thread::sleep(Duration::from_millis(10));

        scheduler.cleanup_expired_cooldowns();

        let cooldowns = scheduler.cooldowns.read();
        assert!(cooldowns.is_empty());
    }

    #[tokio::test]
    async fn test_account_not_selected_during_cooldown() {
        let pool = setup_test_db().await;
        let accounts: Vec<Arc<dyn AccountProvider>> = vec![
            Arc::new(MockAccount::new("test-1", Platform::Claude, 100)),
            Arc::new(MockAccount::new("test-2", Platform::Claude, 50)),
        ];

        let scheduler = UnifiedScheduler::new(accounts, 3600, 300, 3600, pool);

        scheduler.mark_account_unavailable("test-1", "test_reason");

        let request_body = serde_json::json!({});
        let selected = scheduler
            .select_account(Platform::Claude, &request_body)
            .await
            .unwrap();

        assert_eq!(selected.id(), "test-2");
    }

    // ========================================================================
    // New database integration tests
    // ========================================================================

    #[tokio::test]
    async fn test_sticky_session_persisted_to_db() {
        let (scheduler, pool) = setup_scheduler().await;
        let body = serde_json::json!({"system": "test system prompt"});

        // First selection creates sticky session
        let account1 = scheduler
            .select_account(Platform::Claude, &body)
            .await
            .unwrap();

        // Verify session persisted to database
        let session_hash = generate_session_hash(&body).unwrap();
        let db_session = db::get_sticky_session(&pool, &session_hash).await.unwrap();
        assert!(db_session.is_some());
        assert_eq!(db_session.unwrap().0, account1.id());
    }

    #[tokio::test]
    async fn test_sticky_session_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let path_str = path.to_str().unwrap().to_string();
        std::mem::forget(dir);

        let body = serde_json::json!({"system": "test"});

        // First "run"
        let first_account_id = {
            let pool = db::init_database(&path_str).await.unwrap();
            let accounts: Vec<Arc<dyn AccountProvider>> =
                vec![Arc::new(MockAccount::new("acc1", Platform::Claude, 100))];
            let scheduler = UnifiedScheduler::new(accounts, 3600, 300, 3600, pool);
            let account = scheduler
                .select_account(Platform::Claude, &body)
                .await
                .unwrap();
            account.id().to_string()
        };

        // Simulate restart with new scheduler, same database
        let pool = db::init_database(&path_str).await.unwrap();
        let accounts: Vec<Arc<dyn AccountProvider>> = vec![
            Arc::new(MockAccount::new("acc1", Platform::Claude, 100)),
            Arc::new(MockAccount::new("acc2", Platform::Claude, 50)),
        ];
        let scheduler = UnifiedScheduler::new(accounts, 3600, 300, 3600, pool);

        // Should return same account (restored from database)
        let account = scheduler
            .select_account(Platform::Claude, &body)
            .await
            .unwrap();
        assert_eq!(account.id(), first_account_id);
    }

    #[tokio::test]
    async fn test_smart_renewal() {
        let (scheduler, pool) = setup_scheduler().await;
        let body = serde_json::json!({"system": "test"});
        let session_hash = generate_session_hash(&body).unwrap();

        // Insert a session about to expire (100 seconds remaining, threshold is 300)
        db::upsert_sticky_session(&pool, &session_hash, "acc1", 100)
            .await
            .unwrap();

        // Select account should trigger renewal
        scheduler
            .select_account(Platform::Claude, &body)
            .await
            .unwrap();

        // Verify renewed (new remaining time should be ~3600)
        let session = db::get_sticky_session(&pool, &session_hash)
            .await
            .unwrap()
            .unwrap();
        assert!(
            session.1 > 3500,
            "Session should be renewed, got {} seconds",
            session.1
        );
    }

    #[tokio::test]
    async fn test_no_renewal_when_not_needed() {
        let (scheduler, pool) = setup_scheduler().await;
        let body = serde_json::json!({"system": "test"});
        let session_hash = generate_session_hash(&body).unwrap();

        // Insert a session with plenty of time (3000 seconds, threshold is 300)
        db::upsert_sticky_session(&pool, &session_hash, "acc1", 3000)
            .await
            .unwrap();

        // Select account should NOT trigger renewal
        scheduler
            .select_account(Platform::Claude, &body)
            .await
            .unwrap();

        // Verify NOT renewed (remaining time should still be ~3000, not ~3600)
        let session = db::get_sticky_session(&pool, &session_hash)
            .await
            .unwrap()
            .unwrap();
        assert!(
            session.1 < 3100 && session.1 > 2900,
            "Session should NOT be renewed, got {} seconds",
            session.1
        );
    }
}
