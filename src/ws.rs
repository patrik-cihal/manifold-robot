use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

const WS_URL: &str = "wss://api.manifold.markets/ws";

#[derive(Debug, Clone, Serialize)]
struct WsClientMsg {
    #[serde(rename = "type")]
    msg_type: String,
    txid: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    topics: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "lowercase")]
pub enum WsMessage {
    Ack {
        #[allow(dead_code)]
        txid: u64,
        success: bool,
    },
    Broadcast {
        topic: String,
        data: serde_json::Value,
    },
}

/// Wrapper: broadcast data is `{ "contract": Contract, "creator": User }`
#[derive(Debug, Clone, Deserialize)]
pub struct NewContractBroadcast {
    pub contract: ContractData,
    pub creator: CreatorData,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractData {
    pub id: String,
    pub slug: String,
    pub question: String,
    pub outcome_type: String,
    pub mechanism: String,
    pub visibility: String,
    pub created_time: u64,
    pub close_time: Option<u64>,
    pub is_resolved: bool,
    pub volume: Option<f64>,
    // CPMM binary fields
    pub probability: Option<f64>,
    pub p: Option<f64>,
    pub total_liquidity: Option<f64>,
    pub text_description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorData {
    pub id: String,
    pub username: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BetData {
    pub contract_id: String,
    pub prob_before: f64,
    pub prob_after: f64,
}

#[derive(Debug, Deserialize)]
struct NewBetBroadcast {
    bets: Vec<BetData>,
}

#[derive(Debug, Clone)]
pub enum WsEvent {
    Connected,
    NewContract(Box<NewContractBroadcast>),
    NewBet(Box<BetData>),
    Error(String),
    Disconnected,
}

pub async fn run_ws(tx: mpsc::UnboundedSender<WsEvent>) {
    loop {
        if let Err(e) = connect_and_listen(&tx).await {
            let _ = tx.send(WsEvent::Error(format!("WS error: {e}")));
        }
        let _ = tx.send(WsEvent::Disconnected);
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

async fn connect_and_listen(
    tx: &mpsc::UnboundedSender<WsEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (ws_stream, _) = connect_async(WS_URL).await?;
    let (mut write, mut read) = ws_stream.split();

    // Subscribe
    let sub = WsClientMsg {
        msg_type: "subscribe".to_string(),
        txid: 1,
        topics: Some(vec![
            "global/new-contract".to_string(),
            "global/new-bet".to_string(),
        ]),
    };
    write
        .send(Message::Text(serde_json::to_string(&sub)?.into()))
        .await?;

    let _ = tx.send(WsEvent::Connected);

    // JSON ping every 20s to keep connection alive
    let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(20));
    let mut ping_txid: u64 = 100;

    // Staleness: if no message received for 90s, reconnect
    let stale_timeout = std::time::Duration::from_secs(90);

    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                ping_txid += 1;
                let ping = WsClientMsg {
                    msg_type: "ping".to_string(),
                    txid: ping_txid,
                    topics: None,
                };
                if let Ok(json) = serde_json::to_string(&ping) {
                    if write.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }
            msg = tokio::time::timeout(stale_timeout, read.next()) => {
                let msg = match msg {
                    Ok(Some(Ok(m))) => m,
                    Ok(Some(Err(e))) => {
                        let _ = tx.send(WsEvent::Error(format!("WS read error: {e}")));
                        break;
                    }
                    Ok(None) => break, // stream ended
                    Err(_) => {
                        let _ = tx.send(WsEvent::Error("WS stale — no message for 90s".to_string()));
                        break;
                    }
                };
                match msg {
                    Message::Text(text) => {
                        if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text) {
                            match ws_msg {
                                WsMessage::Ack { success, .. } => {
                                    if !success {
                                        let _ = tx.send(WsEvent::Error("Subscription failed".to_string()));
                                    }
                                }
                                WsMessage::Broadcast { topic, data } => {
                                    let event = parse_broadcast(&topic, data);
                                    let _ = tx.send(event);
                                }
                            }
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn parse_broadcast(topic: &str, data: serde_json::Value) -> WsEvent {
    match topic {
        "global/new-contract" => match serde_json::from_value::<NewContractBroadcast>(data) {
            Ok(broadcast) => WsEvent::NewContract(Box::new(broadcast)),
            Err(e) => WsEvent::Error(format!("Failed to parse new contract: {e}")),
        },
        "global/new-bet" => match serde_json::from_value::<NewBetBroadcast>(data) {
            Ok(broadcast) => {
                if let Some(bet) = broadcast.bets.into_iter().next() {
                    WsEvent::NewBet(Box::new(bet))
                } else {
                    WsEvent::Error("Empty bets array in new-bet broadcast".to_string())
                }
            }
            Err(e) => WsEvent::Error(format!("Failed to parse new bet: {e}")),
        },
        _ => WsEvent::Error(format!("Unknown topic: {topic}")),
    }
}
