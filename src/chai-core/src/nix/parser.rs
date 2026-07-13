use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct NixPlan {
    pub packages: Vec<String>,
    #[serde(default)]
    pub services: Vec<NixService>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NixService {
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

const fn default_enabled() -> bool {
    true
}

pub fn parse_llm_output(input: &str) -> Result<NixPlan, ParseError> {
    let plan: NixPlan = serde_json::from_str(input)?;
    if plan.packages.is_empty() && plan.services.is_empty() {
        return Err(ParseError::EmptyPlan);
    }
    Ok(plan)
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("LLM returned empty plan: no packages or services")]
    EmptyPlan,
}
