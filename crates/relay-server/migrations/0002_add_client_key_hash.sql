-- Add client_api_key_hash column for tracking usage by downstream API key
ALTER TABLE usage_stats ADD COLUMN client_api_key_hash TEXT NOT NULL DEFAULT 'legacy';
CREATE INDEX IF NOT EXISTS idx_usage_client_key ON usage_stats(client_api_key_hash, created_at);
