use crate::domain::rules::{RenameRule, RuleConfig, RuleError, build_rule};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct FileEntry {
    #[serde(skip)]
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum PreviewRow {
    Proposed { name: String },
    Unchanged,
    Error { reason: &'static str },
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum Preview {
    Ready { rows: Vec<PreviewRow> },
    Invalid { errors: Vec<RuleError> },
    Stale,
}

pub fn generate_preview(files: &[FileEntry], config: &RuleConfig) -> Preview {
    match build_rule(config) {
        Ok(rule) => Preview::Ready {
            rows: files
                .iter()
                .map(|file| match rule.apply(&file.name) {
                    Ok(Some(name)) => PreviewRow::Proposed { name },
                    Ok(None) => PreviewRow::Unchanged,
                    Err(reason) => PreviewRow::Error { reason },
                })
                .collect(),
        },
        Err(errors) => Preview::Invalid { errors },
    }
}
