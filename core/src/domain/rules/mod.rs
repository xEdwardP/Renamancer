mod episode;
mod template;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
pub struct RuleConfig {
    pub season: String,
    pub digits: String,
    pub pattern: String,
    pub template: String,
}

impl Default for RuleConfig {
    fn default() -> Self {
        Self {
            season: "1".into(),
            digits: "2".into(),
            pattern: "([0-9]+)[^0-9]*$".into(),
            template: "S{temporada}E{episodio}{ext}".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleField {
    Season,
    Digits,
    Pattern,
    Template,
}

#[derive(Debug, Serialize)]
pub struct RuleError {
    pub field: RuleField,
    pub message: &'static str,
}

pub trait RenameRule {
    fn apply(&self, name: &str) -> Result<Option<String>, &'static str>;
}

pub fn build_rule(config: &RuleConfig) -> Result<impl RenameRule, Vec<RuleError>> {
    episode::EpisodeRule::build(config)
}
