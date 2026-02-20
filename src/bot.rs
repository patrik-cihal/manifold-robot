use crate::api::{BetRequest, ManifoldClient};
use crate::ws::{BetData, NewContractBroadcast, WsEvent};
use crate::xai::{self, XaiClient};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum BotLogEntry {
    Info(String),
    Trade(String),
    Error(String),
}

#[derive(Clone)]
pub struct BotConfig {
    pub bet_amount: f64,
    /// Bet size for existing markets discovered via new-bet events.
    pub reversion_amount: f64,
    /// Minimum absolute edge (prediction vs market) to place a bet.
    pub min_edge: f64,
    /// Minimum pool liquidity (mana) to consider a market worth trading.
    pub min_liquidity: f64,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            bet_amount: 10.0,
            reversion_amount: 25.0,
            min_edge: 0.10,
            min_liquidity: 100.0,
        }
    }
}

const CACHE_TTL_SECS: u64 = 24 * 60 * 60;

fn cache_file_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("manifold-domination")
        .join("analyzed_cache.json")
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn load_cache() -> HashMap<String, u64> {
    let path = cache_file_path();
    let data = match std::fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return HashMap::new(),
    };
    let cache: HashMap<String, u64> = match serde_json::from_str(&data) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let now = now_epoch_secs();
    cache
        .into_iter()
        .filter(|(_, ts)| now.saturating_sub(*ts) < CACHE_TTL_SECS)
        .collect()
}

fn save_cache(cache: &HashMap<String, u64>) {
    let path = cache_file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, serde_json::to_string(cache).unwrap_or_default());
}

pub async fn run_bot(
    manifold: ManifoldClient,
    xai: XaiClient,
    mut ws_rx: mpsc::UnboundedReceiver<WsEvent>,
    log_tx: mpsc::UnboundedSender<BotLogEntry>,
    config: BotConfig,
) {
    let _ = log_tx.send(BotLogEntry::Info(format!(
        "Bot started (M${:.0}/new, M${:.0}/reversion, {:.0}% min edge, M${:.0} min liquidity)",
        config.bet_amount,
        config.reversion_amount,
        config.min_edge * 100.0,
        config.min_liquidity,
    )));

    // Track which markets we've already analyzed (market_id -> epoch secs), persisted to disk
    let mut analyzed_cache = load_cache();

    while let Some(event) = ws_rx.recv().await {
        match event {
            WsEvent::Connected => {
                let _ = log_tx.send(BotLogEntry::Info("WebSocket connected".to_string()));
            }
            WsEvent::Disconnected => {
                let _ = log_tx.send(BotLogEntry::Info(
                    "WebSocket disconnected, reconnecting...".to_string(),
                ));
            }
            WsEvent::NewContract(broadcast) => {
                let contract = &broadcast.contract;
                let creator = &broadcast.creator;

                if contract.outcome_type == "BINARY" {
                    let liquidity = contract.total_liquidity.unwrap_or(0.0);
                    if liquidity < config.min_liquidity {
                        let _ = log_tx.send(BotLogEntry::Info(format!(
                            "Skipping low-liquidity market (M${:.0}): \"{}\"",
                            liquidity, contract.question
                        )));
                        continue;
                    }

                    let _ = log_tx.send(BotLogEntry::Info(format!(
                        "New binary market (M${:.0} liq): \"{}\" by {}",
                        liquidity, contract.question, creator.username
                    )));
                    // Mark as analyzed so bet events don't re-trigger
                    analyzed_cache.insert(contract.id.clone(), now_epoch_secs());
                    save_cache(&analyzed_cache);
                    let manifold = manifold.clone();
                    let xai = xai.clone();
                    let log_tx = log_tx.clone();
                    let broadcast = broadcast.clone();
                    let config = config.clone();
                    tokio::spawn(async move {
                        handle_new_market(&manifold, &xai, &log_tx, &broadcast, &config).await;
                    });
                } else {
                    let _ = log_tx.send(BotLogEntry::Info(format!(
                        "Skipping non-binary market: \"{}\" [{}]",
                        contract.question, contract.outcome_type
                    )));
                }
            }
            WsEvent::NewBet(bet) => {
                // Evict stale cache entries periodically
                let now = now_epoch_secs();
                analyzed_cache.retain(|_, ts| now.saturating_sub(*ts) < CACHE_TTL_SECS);

                if analyzed_cache.contains_key(&bet.contract_id) {
                    continue;
                }
                analyzed_cache.insert(bet.contract_id.clone(), now);
                save_cache(&analyzed_cache);

                let manifold = manifold.clone();
                let xai = xai.clone();
                let log_tx = log_tx.clone();
                let config = config.clone();
                let bet = *bet;
                tokio::spawn(async move {
                    handle_bet_triggered(&manifold, &xai, &log_tx, &bet, &config).await;
                });
            }
            WsEvent::Error(e) => {
                let _ = log_tx.send(BotLogEntry::Error(e));
            }
        }
    }
}

async fn handle_new_market(
    manifold: &ManifoldClient,
    xai: &XaiClient,
    log_tx: &mpsc::UnboundedSender<BotLogEntry>,
    broadcast: &NewContractBroadcast,
    config: &BotConfig,
) {
    let question = &broadcast.contract.question;
    let contract_id = &broadcast.contract.id;

    let _ = log_tx.send(BotLogEntry::Info(format!(
        "Researching \"{question}\"...",
    )));

    let description = broadcast.contract.text_description.as_deref();
    let result = match xai.research_market(question, description).await {
        Ok(r) => r,
        Err(e) => {
            let _ = log_tx.send(BotLogEntry::Error(format!(
                "xAI research failed for \"{question}\": {e}",
            )));
            return;
        }
    };

    let prediction = match xai::parse_prediction(&result.text) {
        Some(xai::PredictionResult::Predict(p)) => p,
        Some(xai::PredictionResult::Skip(reason)) => {
            let _ = log_tx.send(BotLogEntry::Info(format!(
                "Skipping unevaluable market: \"{question}\" | {reason}",
            )));
            return;
        }
        None => {
            let _ = log_tx.send(BotLogEntry::Error(format!(
                "Could not parse prediction for \"{question}\"",
            )));
            let truncated = &result.text[..result.text.len().min(300)];
            let _ = log_tx.send(BotLogEntry::Info(format!("xAI response: {truncated}")));
            return;
        }
    };

    let market_prob = broadcast.contract.probability.unwrap_or(0.5);
    let edge = prediction.probability - market_prob;
    let abs_edge = edge.abs();

    let reasoning = if prediction.reasoning.is_empty() {
        "No reasoning provided".to_string()
    } else {
        prediction.reasoning
    };

    if abs_edge < config.min_edge {
        let _ = log_tx.send(BotLogEntry::Info(format!(
            "[{question}] {:.0}% (market {:.0}%), edge {:.1}% < {:.0}% min — skipping | {reasoning}",
            prediction.probability * 100.0,
            market_prob * 100.0,
            abs_edge * 100.0,
            config.min_edge * 100.0,
        )));
        return;
    }

    let (outcome, limit_prob) = if edge > 0.0 {
        ("YES", prediction.probability)
    } else {
        ("NO", prediction.probability)
    };

    let _ = log_tx.send(BotLogEntry::Info(format!(
        "[{question}] {:.0}% (market {:.0}%) -> {outcome} limit@{:.0}% | {reasoning}",
        prediction.probability * 100.0,
        market_prob * 100.0,
        limit_prob * 100.0,
    )));

    // Clamp limit_prob to valid range (1-99%)
    let limit_prob = limit_prob.clamp(0.01, 0.99);

    let bet = BetRequest {
        contract_id: contract_id.clone(),
        amount: config.bet_amount,
        outcome: outcome.to_string(),
        limit_prob: Some(limit_prob),
    };

    match manifold.place_bet(&bet).await {
        Ok(resp) => {
            let filled = resp.amount.unwrap_or(0.0);
            let _ = log_tx.send(BotLogEntry::Trade(format!(
                "BET PLACED: {outcome} M${:.0} on \"{question}\" limit@{:.0}% (filled M${filled:.0})",
                config.bet_amount,
                limit_prob * 100.0,
            )));
        }
        Err(e) => {
            let _ = log_tx.send(BotLogEntry::Error(format!(
                "Failed to place bet on \"{question}\": {e}",
            )));
        }
    }
}

async fn handle_bet_triggered(
    manifold: &ManifoldClient,
    xai: &XaiClient,
    log_tx: &mpsc::UnboundedSender<BotLogEntry>,
    bet_data: &BetData,
    config: &BotConfig,
) {
    let market = match manifold.get_market(&bet_data.contract_id).await {
        Ok(m) => m,
        Err(e) => {
            let _ = log_tx.send(BotLogEntry::Error(format!(
                "Failed to fetch market {}: {e}",
                bet_data.contract_id
            )));
            return;
        }
    };

    if market.is_resolved || market.outcome_type != "BINARY" {
        return;
    }

    let liquidity = market.total_liquidity.unwrap_or(0.0);
    if liquidity < config.min_liquidity {
        return;
    }

    let question = &market.question;
    let _ = log_tx.send(BotLogEntry::Info(format!(
        "Analyzing market (bet-triggered, M${liquidity:.0} liq): \"{question}\""
    )));

    let description = market.text_description.as_deref();
    let result = match xai.research_market(question, description).await {
        Ok(r) => r,
        Err(e) => {
            let _ = log_tx.send(BotLogEntry::Error(format!(
                "xAI research failed for \"{question}\": {e}"
            )));
            return;
        }
    };

    let prediction = match xai::parse_prediction(&result.text) {
        Some(xai::PredictionResult::Predict(p)) => p,
        Some(xai::PredictionResult::Skip(reason)) => {
            let _ = log_tx.send(BotLogEntry::Info(format!(
                "Skipping unevaluable market: \"{question}\" | {reason}",
            )));
            return;
        }
        None => {
            let _ = log_tx.send(BotLogEntry::Error(format!(
                "Could not parse prediction for \"{question}\""
            )));
            return;
        }
    };

    let market_prob = market.probability.unwrap_or(0.5);
    let edge = prediction.probability - market_prob;
    let abs_edge = edge.abs();

    let reasoning = if prediction.reasoning.is_empty() {
        "No reasoning provided".to_string()
    } else {
        prediction.reasoning
    };

    if abs_edge < config.min_edge {
        let _ = log_tx.send(BotLogEntry::Info(format!(
            "[bet-triggered] [{question}] {:.0}% (market {:.0}%), edge {:.1}% < {:.0}% min — skipping | {reasoning}",
            prediction.probability * 100.0,
            market_prob * 100.0,
            abs_edge * 100.0,
            config.min_edge * 100.0,
        )));
        return;
    }

    let (outcome, limit_prob) = if edge > 0.0 {
        ("YES", prediction.probability)
    } else {
        ("NO", prediction.probability)
    };

    let _ = log_tx.send(BotLogEntry::Info(format!(
        "[bet-triggered] [{question}] {:.0}% (market {:.0}%) -> {outcome} limit@{:.0}% | {reasoning}",
        prediction.probability * 100.0,
        market_prob * 100.0,
        limit_prob * 100.0,
    )));

    let limit_prob = limit_prob.clamp(0.01, 0.99);

    let bet = BetRequest {
        contract_id: bet_data.contract_id.clone(),
        amount: config.reversion_amount,
        outcome: outcome.to_string(),
        limit_prob: Some(limit_prob),
    };

    match manifold.place_bet(&bet).await {
        Ok(resp) => {
            let filled = resp.amount.unwrap_or(0.0);
            let _ = log_tx.send(BotLogEntry::Trade(format!(
                "BET PLACED (reversion): {outcome} M${:.0} on \"{question}\" limit@{:.0}% (filled M${filled:.0})",
                config.reversion_amount,
                limit_prob * 100.0,
            )));
        }
        Err(e) => {
            let _ = log_tx.send(BotLogEntry::Error(format!(
                "Failed to place bet on \"{question}\": {e}"
            )));
        }
    }
}
