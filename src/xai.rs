use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct XaiRequest {
    model: String,
    input: Vec<InputMessage>,
    tools: Vec<Tool>,
    text: TextFormat,
}

#[derive(Serialize)]
struct TextFormat {
    format: FormatSpec,
}

#[derive(Serialize)]
struct FormatSpec {
    #[serde(rename = "type")]
    format_type: String,
    name: String,
    schema: serde_json::Value,
}

#[derive(Serialize)]
struct InputMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct Tool {
    #[serde(rename = "type")]
    tool_type: String,
}

#[derive(Deserialize, Debug)]
pub struct XaiResponse {
    pub output: Option<Vec<OutputItem>>,
    pub error: Option<ApiError>,
}

#[derive(Deserialize, Debug)]
pub struct ApiError {
    pub message: String,
}

#[derive(Deserialize, Debug)]
pub struct OutputItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub content: Option<Vec<ContentBlock>>,
}

#[derive(Deserialize, Debug)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: Option<String>,
}

#[derive(Clone)]
pub struct XaiClient {
    http: reqwest::Client,
    api_key: String,
}

pub struct SearchResult {
    pub text: String,
}

impl SearchResult {
    fn from_response(resp: XaiResponse) -> Self {
        let mut text = String::new();
        if let Some(output) = &resp.output {
            for item in output {
                if item.item_type == "message" {
                    if let Some(content) = &item.content {
                        for block in content {
                            if block.block_type == "output_text" {
                                if let Some(t) = &block.text {
                                    text.push_str(t);
                                }
                            }
                        }
                    }
                }
            }
        }
        Self { text }
    }
}

impl XaiClient {
    pub fn new(api_key: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key,
        }
    }

    pub async fn research_market(
        &self,
        question: &str,
        description: Option<&str>,
    ) -> Result<SearchResult, Box<dyn std::error::Error + Send + Sync>> {
        let description_section = match description {
            Some(desc) if !desc.is_empty() => format!(
                "\n\nResolution criteria / description:\n\"{desc}\""
            ),
            _ => String::new(),
        };

        let prompt = format!(
            "Search X (Twitter) for recent posts, news, and discussion about the following \
             prediction market question. Focus on finding concrete evidence: official announcements, \
             credible reporting, expert opinions, and sentiment from informed accounts.\n\n\
             Based ONLY on what you find on X, estimate the probability (0-100) that this \
             resolves YES. If you find little or no relevant information on X, say so and \
             give a low-confidence estimate near 50.\n\n\
             If this market is subjective, personal, not objectively resolvable, \
             or depends on information you cannot access (e.g. private metrics, personal decisions, \
             inside knowledge), set action to \"skip\".\n\n\
             Question: \"{question}\"{description_section}"
        );

        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["predict", "skip"],
                    "description": "Whether to predict or skip this market"
                },
                "probability": {
                    "type": "number",
                    "description": "Predicted probability 0-100 that the market resolves YES. Required when action is predict."
                },
                "reasoning": {
                    "type": "string",
                    "description": "One sentence summary of key evidence or why the market was skipped"
                }
            },
            "required": ["action", "reasoning"],
            "additionalProperties": false
        });

        let request = XaiRequest {
            model: "grok-4-1-fast".to_string(),
            input: vec![InputMessage {
                role: "user".to_string(),
                content: prompt,
            }],
            tools: vec![Tool {
                tool_type: "x_search".to_string(),
            }],
            text: TextFormat {
                format: FormatSpec {
                    format_type: "json_schema".to_string(),
                    name: "market_prediction".to_string(),
                    schema,
                },
            },
        };

        let resp = self
            .http
            .post("https://api.x.ai/v1/responses")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(120))
            .json(&request)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await?;
            return Err(format!("xAI API error {status}: {body}").into());
        }

        let response: XaiResponse = resp.json().await?;

        if let Some(err) = &response.error {
            return Err(format!("xAI error: {}", err.message).into());
        }

        Ok(SearchResult::from_response(response))
    }
}

pub struct Prediction {
    pub probability: f64,
    pub reasoning: String,
}

pub enum PredictionResult {
    Predict(Prediction),
    Skip(String),
}

#[derive(Deserialize)]
struct JsonPrediction {
    action: String,
    probability: Option<f64>,
    reasoning: String,
}

/// Parse structured JSON prediction from the response text.
pub fn parse_prediction(text: &str) -> Option<PredictionResult> {
    let parsed: JsonPrediction = serde_json::from_str(text).ok()?;

    match parsed.action.as_str() {
        "skip" => Some(PredictionResult::Skip(parsed.reasoning)),
        "predict" => {
            let pct = parsed.probability?;
            if !(0.0..=100.0).contains(&pct) {
                return None;
            }
            Some(PredictionResult::Predict(Prediction {
                probability: pct / 100.0,
                reasoning: parsed.reasoning,
            }))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_prediction() {
        let r = parse_prediction(
            r#"{"action":"predict","probability":65,"reasoning":"Strong evidence"}"#,
        )
        .unwrap();
        match r {
            PredictionResult::Predict(p) => {
                assert_eq!(p.probability, 0.65);
                assert_eq!(p.reasoning, "Strong evidence");
            }
            PredictionResult::Skip(_) => panic!("expected Predict"),
        }

        let r = parse_prediction(
            r#"{"action":"predict","probability":10,"reasoning":""}"#,
        )
        .unwrap();
        match r {
            PredictionResult::Predict(p) => {
                assert_eq!(p.probability, 0.10);
                assert!(p.reasoning.is_empty());
            }
            PredictionResult::Skip(_) => panic!("expected Predict"),
        }

        assert!(parse_prediction("not json at all").is_none());

        let r = parse_prediction(
            r#"{"action":"skip","reasoning":"Subjective market"}"#,
        )
        .unwrap();
        match r {
            PredictionResult::Skip(reason) => assert_eq!(reason, "Subjective market"),
            PredictionResult::Predict(_) => panic!("expected Skip"),
        }
    }
}
