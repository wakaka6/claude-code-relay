use parking_lot::RwLock;
use relay_core::{generate_session_hash, AccountProvider, Platform, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

pub struct StickySession {
    account_id: String,
    expires_at: Instant,
}

pub struct AccountCooldown {
    until: Instant,
    reason: String,
}

pub struct AccountUsage {
    last_used: Instant,
    request_count: u64,
}

pub struct UnifiedScheduler {
    accounts: Vec<Arc<dyn AccountProvider>>,
    sticky_sessions: RwLock<HashMap<String, StickySession>>,
    cooldowns: RwLock<HashMap<String, AccountCooldown>>,
    usage: RwLock<HashMap<String, AccountUsage>>,
    sticky_ttl: Duration,
    renewal_threshold: Duration,
}

impl UnifiedScheduler {
    pub fn new(
        accounts: Vec<Arc<dyn AccountProvider>>,
        sticky_ttl_secs: u64,
        renewal_threshold_secs: u64,
    ) -> Self {
        Self {
            accounts,
            sticky_sessions: RwLock::new(HashMap::new()),
            cooldowns: RwLock::new(HashMap::new()),
            usage: RwLock::new(HashMap::new()),
            sticky_ttl: Duration::from_secs(sticky_ttl_secs),
            renewal_threshold: Duration::from_secs(renewal_threshold_secs),
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
        let until = Instant::now() + Duration::from_secs(3600);
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
            "Account marked as unavailable for 1 hour"
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

    pub fn select_account(
        &self,
        platform: Platform,
        request_body: &serde_json::Value,
    ) -> Result<Arc<dyn AccountProvider>> {
        self.select_account_excluding(platform, request_body, &HashSet::new())
    }

    pub fn select_account_excluding(
        &self,
        platform: Platform,
        request_body: &serde_json::Value,
        excluded: &HashSet<String>,
    ) -> Result<Arc<dyn AccountProvider>> {
        let session_hash = generate_session_hash(request_body);

        if let Some(ref hash) = session_hash {
            if let Some(account) = self.get_sticky_account(hash, platform, excluded) {
                debug!(session_hash = %hash, account_id = account.id(), "Using sticky session account");
                self.record_account_used(account.id());
                return Ok(account);
            }
        }

        let account = self.select_available_account(platform, excluded)?;

        if let Some(hash) = session_hash {
            self.set_sticky_session(&hash, account.id());
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

    fn get_sticky_account(
        &self,
        session_hash: &str,
        platform: Platform,
        excluded: &HashSet<String>,
    ) -> Option<Arc<dyn AccountProvider>> {
        let now = Instant::now();

        {
            let sessions = self.sticky_sessions.read();
            if let Some(session) = sessions.get(session_hash) {
                if now < session.expires_at {
                    if excluded.contains(&session.account_id) {
                        return None;
                    }
                    if self.is_account_in_cooldown(&session.account_id) {
                        return None;
                    }

                    let account = self.accounts.iter().find(|a| {
                        a.id() == session.account_id
                            && a.platform() == platform
                            && a.is_available()
                    });

                    if let Some(account) = account {
                        let remaining = session.expires_at.duration_since(now);
                        if remaining < self.renewal_threshold {
                            drop(sessions);
                            self.renew_sticky_session(session_hash);
                        }
                        return Some(account.clone());
                    }
                }
            }
        }

        let mut sessions = self.sticky_sessions.write();
        sessions.remove(session_hash);
        None
    }

    fn set_sticky_session(&self, session_hash: &str, account_id: &str) {
        let mut sessions = self.sticky_sessions.write();
        sessions.insert(
            session_hash.to_string(),
            StickySession {
                account_id: account_id.to_string(),
                expires_at: Instant::now() + self.sticky_ttl,
            },
        );
    }

    fn renew_sticky_session(&self, session_hash: &str) {
        let mut sessions = self.sticky_sessions.write();
        if let Some(session) = sessions.get_mut(session_hash) {
            session.expires_at = Instant::now() + self.sticky_ttl;
            debug!(session_hash = %session_hash, "Renewed sticky session");
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

    pub fn cleanup_expired_sessions(&self) {
        let now = Instant::now();

        {
            let mut sessions = self.sticky_sessions.write();
            let before = sessions.len();
            sessions.retain(|_, session| now < session.expires_at);
            let removed = before - sessions.len();
            if removed > 0 {
                debug!(removed = removed, "Cleaned up expired sticky sessions");
            }
        }

        {
            let mut cooldowns = self.cooldowns.write();
            let before = cooldowns.len();
            cooldowns.retain(|_, cooldown| now < cooldown.until);
            let removed = before - cooldowns.len();
            if removed > 0 {
                debug!(removed = removed, "Cleaned up expired account cooldowns");
            }
        }
    }

    pub fn get_accounts_by_platform(&self, platform: Platform) -> Vec<Arc<dyn AccountProvider>> {
        self.accounts
            .iter()
            .filter(|a| a.platform() == platform)
            .cloned()
            .collect()
    }

    pub fn get_all_accounts(&self) -> &[Arc<dyn AccountProvider>] {
        &self.accounts
    }
}
