use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct XaiRequest {
    model: String,
    input: Vec<InputMessage>,
    tools: Vec<Tool>,
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
    ) -> Result<SearchResult, Box<dyn std::error::Error + Send + Sync>> {
        let prompt = format!(
            "Search X (Twitter) for recent posts, news, and discussion about the following \
             prediction market question. Focus on finding concrete evidence: official announcements, \
             credible reporting, expert opinions, and sentiment from informed accounts.\n\n\
             Based ONLY on what you find on X, estimate the probability (0-100%) that this \
             resolves YES. If you find little or no relevant information on X, say so and \
             give a low-confidence estimate near 50%.\n\n\
             You MUST end your response with exactly these two lines:\n\
             REASONING: <one sentence summary of the key evidence found>\n\
             PROBABILITY: XX%\n\n\
             Question: \"{question}\""
        );

        let request = XaiRequest {
            model: "grok-4-1-fast".to_string(),
            input: vec![InputMessage {
                role: "user".to_string(),
                content: prompt,
            }],
            tools: vec![Tool {
                tool_type: "x_search".to_string(),
            }],
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

/// Parse "REASONING: ..." and "PROBABILITY: XX%" from the response text.
pub fn parse_prediction(text: &str) -> Option<Prediction> {
    let mut probability = None;
    let mut reasoning = String::new();

    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("PROBABILITY:") {
            let rest = rest.trim().trim_end_matches('%').trim();
            if let Ok(pct) = rest.parse::<f64>() {
                if (0.0..=100.0).contains(&pct) {
                    probability = Some(pct / 100.0);
                }
            }
        } else if let Some(rest) = line.strip_prefix("REASONING:") {
            reasoning = rest.trim().to_string();
        }
    }

    probability.map(|p| Prediction {
        probability: p,
        reasoning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_prediction() {
        let p = parse_prediction("blah\nREASONING: Strong evidence\nPROBABILITY: 65%").unwrap();
        assert_eq!(p.probability, 0.65);
        assert_eq!(p.reasoning, "Strong evidence");

        let p = parse_prediction("PROBABILITY: 10%\n").unwrap();
        assert_eq!(p.probability, 0.10);
        assert!(p.reasoning.is_empty());

        assert!(parse_prediction("no probability here").is_none());
    }
}
