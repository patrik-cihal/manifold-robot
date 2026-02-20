use serde::{Deserialize, Serialize};

const BASE_URL: &str = "https://api.manifold.markets/v0";

#[derive(Clone)]
pub struct ManifoldClient {
    api_key: String,
    client: reqwest::Client,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub username: String,
    pub name: String,
    pub balance: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Market {
    pub id: String,
    pub question: String,
    pub url: String,
    pub probability: Option<f64>,
    pub outcome_type: String,
    pub mechanism: String,
    pub is_resolved: bool,
    pub close_time: Option<u64>,
    pub creator_username: String,
    pub total_liquidity: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BetRequest {
    pub contract_id: String,
    pub amount: f64,
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_prob: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BetResponse {
    #[serde(alias = "betId")]
    pub bet_id: Option<String>,
    pub amount: Option<f64>,
    pub outcome: Option<String>,
    pub contract_id: Option<String>,
}

impl ManifoldClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    pub async fn get_me(&self) -> Result<User, reqwest::Error> {
        self.client
            .get(format!("{BASE_URL}/me"))
            .header("Authorization", format!("Key {}", self.api_key))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
    }

    pub async fn get_market(&self, id: &str) -> Result<Market, reqwest::Error> {
        self.client
            .get(format!("{BASE_URL}/market/{id}"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
    }

    pub async fn place_bet(
        &self,
        request: &BetRequest,
    ) -> Result<BetResponse, Box<dyn std::error::Error + Send + Sync>> {
        let resp = self
            .client
            .post(format!("{BASE_URL}/bet"))
            .header("Authorization", format!("Key {}", self.api_key))
            .json(request)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Bet API error {status}: {body}").into());
        }

        Ok(resp.json().await?)
    }
}
