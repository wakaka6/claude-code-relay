mod config;
mod db;
mod middleware;
mod routes;
mod scheduler;

use axum::{
    middleware as axum_middleware,
    routing::{get, post},
    Router,
};
use clap::Parser;
use relay_claude::{ClaudeApiAccount, ClaudeOAuthAccount, ClaudeRelay};
use relay_core::AccountProvider;
use relay_gemini::{GeminiAccount, GeminiRelay};
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::{AccountConfig, Config};
use middleware::ApiKeyValidator;
use relay_core::Platform;
use routes::{ClaudeRouteState, GeminiRouteState, OpenAIRouteState};
use scheduler::UnifiedScheduler;

#[derive(Parser)]
#[command(name = "claude-relay")]
#[command(about = "Claude Relay Service - Multi-platform AI API relay")]
struct Args {
    #[arg(short, long, default_value = "config.toml")]
    config: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let config = match Config::load(&args.config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load config: {}", e);
            std::process::exit(1);
        }
    };

    init_tracing(&config.server.log_level);

    info!(config_path = %args.config, "Starting Claude Relay Service");
    info!(api_keys_count = config.api_keys.len(), api_keys = ?config.api_keys, "Loaded API keys config");

    let pool = match db::init_database(&config.server.database_path).await {
        Ok(p) => p,
        Err(e) => {
            error!(error = %e, "Failed to initialize database");
            std::process::exit(1);
        }
    };

    let accounts = build_accounts(&config);

    let claude_count = accounts
        .iter()
        .filter(|a| a.platform() == Platform::Claude)
        .count();
    let gemini_count = accounts
        .iter()
        .filter(|a| a.platform() == Platform::Gemini)
        .count();
    let codex_count = accounts
        .iter()
        .filter(|a| a.platform() == Platform::Codex)
        .count();

    info!(
        claude_accounts = claude_count,
        gemini_accounts = gemini_count,
        codex_accounts = codex_count,
        total_accounts = accounts.len(),
        "Loaded accounts"
    );

    if claude_count == 0 {
        info!("No Claude accounts configured - Claude/OpenAI endpoints will return errors");
    }
    if gemini_count == 0 {
        info!("No Gemini accounts configured - Gemini endpoints will return errors");
    }
    if codex_count == 0 {
        info!("No Codex accounts configured - OpenAI Responses endpoints will return errors");
    }

    let scheduler = Arc::new(UnifiedScheduler::new(
        accounts,
        config.session.sticky_ttl_seconds,
        config.session.renewal_threshold_seconds,
        config.session.unavailable_cooldown_seconds,
        pool.clone(),
    ));

    let scheduler_cleanup = scheduler.clone();
    let cleanup_pool = pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            scheduler_cleanup.cleanup_expired_cooldowns();
            if let Err(e) = db::cleanup_expired_sessions(&cleanup_pool).await {
                error!(error = %e, "Failed to cleanup expired sessions");
            }
        }
    });

    let api_key_validator = Arc::new(ApiKeyValidator::new(config.api_keys.clone()));

    if api_key_validator.is_empty() {
        info!("No API keys configured - all requests will be anonymous");
    } else {
        info!(count = config.api_keys.len(), "API key authentication enabled");
    }

    let claude_relay = Arc::new(ClaudeRelay::new());
    let gemini_relay = Arc::new(GeminiRelay::new());
    let codex_relay = Arc::new(relay_codex::CodexRelay::new());

    let claude_state = Arc::new(ClaudeRouteState {
        scheduler: scheduler.clone(),
        relay: claude_relay.clone(),
        db_pool: pool.clone(),
    });

    let gemini_state = Arc::new(GeminiRouteState {
        scheduler: scheduler.clone(),
        relay: gemini_relay,
        db_pool: pool.clone(),
    });

    let openai_state = Arc::new(OpenAIRouteState {
        scheduler: scheduler.clone(),
        relay: claude_relay,
        db_pool: pool.clone(),
    });

    let codex_state = Arc::new(routes::CodexRouteState {
        scheduler: scheduler.clone(),
        relay: codex_relay,
        db_pool: pool.clone(),
    });

    let claude_routes = Router::new()
        .route("/v1/messages", post(routes::claude::messages))
        .route("/api/v1/messages", post(routes::claude::messages))
        .route("/claude/v1/messages", post(routes::claude::messages))
        .route("/v1/models", get(routes::claude::models))
        .route("/api/v1/models", get(routes::claude::models))
        .with_state(claude_state);

    let gemini_routes = Router::new()
        .route(
            "/gemini/v1/models/*model_method",
            post(routes::gemini::generate_content),
        )
        .route("/gemini/v1/models", get(routes::gemini::models))
        .with_state(gemini_state);

    let openai_routes = Router::new()
        .route(
            "/openai/v1/chat/completions",
            post(routes::openai::chat_completions),
        )
        .route("/openai/v1/models", get(routes::openai::models))
        .with_state(openai_state);

    let codex_routes = Router::new()
        .route("/openai/v1/responses", post(routes::codex::responses))
        .route("/v1/responses", post(routes::codex::responses))
        .with_state(codex_state);

    let app = Router::new()
        .merge(claude_routes)
        .merge(gemini_routes)
        .merge(openai_routes)
        .merge(codex_routes)
        .route("/health", get(health_check))
        .layer(axum_middleware::from_fn_with_state(
            api_key_validator,
            middleware::auth_middleware,
        ));

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = TcpListener::bind(&addr).await.unwrap();

    info!(address = %addr, "Server listening");

    axum::serve(listener, app).await.unwrap();
}

fn build_accounts(config: &Config) -> Vec<Arc<dyn AccountProvider>> {
    config
        .accounts
        .iter()
        .map(|acc| -> Arc<dyn AccountProvider> {
            match acc {
                AccountConfig::ClaudeOauth {
                    id,
                    name,
                    priority,
                    enabled,
                    refresh_token,
                    api_url,
                    proxy,
                } => Arc::new(ClaudeOAuthAccount::new(
                    id.clone(),
                    name.clone(),
                    *priority,
                    *enabled,
                    refresh_token.clone(),
                    api_url.clone(),
                    proxy.clone(),
                )),
                AccountConfig::ClaudeApi {
                    id,
                    name,
                    priority,
                    enabled,
                    api_key,
                    api_url,
                    proxy,
                } => Arc::new(ClaudeApiAccount::new(
                    id.clone(),
                    name.clone(),
                    *priority,
                    *enabled,
                    api_key.clone(),
                    api_url.clone(),
                    proxy.clone(),
                )),
                AccountConfig::Gemini {
                    id,
                    name,
                    priority,
                    enabled,
                    refresh_token,
                    api_url,
                    proxy,
                } => Arc::new(GeminiAccount::new(
                    id.clone(),
                    name.clone(),
                    *priority,
                    *enabled,
                    refresh_token.clone(),
                    api_url.clone(),
                    proxy.clone(),
                )),
                AccountConfig::OpenaiResponses {
                    id,
                    name,
                    priority,
                    enabled,
                    api_key,
                    api_url,
                    proxy,
                } => Arc::new(relay_codex::CodexAccount::new(
                    id.clone(),
                    name.clone(),
                    *priority,
                    *enabled,
                    api_key.clone(),
                    api_url.clone(),
                    proxy.clone(),
                )),
            }
        })
        .collect()
}

fn init_tracing(level: &str) {
    let filter = match level.to_lowercase().as_str() {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "info" => tracing::Level::INFO,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_thread_ids(false)
                .with_file(false)
                .with_line_number(false),
        )
        .with(tracing_subscriber::filter::LevelFilter::from_level(filter))
        .init();
}

async fn health_check() -> &'static str {
    "OK"
}
