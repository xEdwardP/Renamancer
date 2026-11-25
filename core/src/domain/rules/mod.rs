mod episode;
mod numbering;
mod regex;
mod template;
mod text;

use crate::domain::file_name;
use episode::EpisodeRule;
use numbering::NumberingRule;
use regex::RegexRule;
use serde::{Deserialize, Serialize};
pub use text::Capitalization;
use text::TextRule;

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

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Extension {
    #[default]
    Keep,
    Remove,
    Change {
        value: String,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EditRule {
    Episode {
        #[serde(flatten)]
        rule: RuleConfig,
        extension: Extension,
    },
    Regex {
        pattern: String,
        replacement: String,
        ignore_case: bool,
        extension: Extension,
    },
    Numbering {
        start: String,
        step: String,
        width: String,
        template: String,
        extension: Extension,
    },
    Text {
        search: String,
        replacement: String,
        prefix: String,
        suffix: String,
        capitalization: Capitalization,
        extension: Extension,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleField {
    Season,
    Digits,
    Pattern,
    Template,
    Start,
    Step,
    Width,
    Extension,
}

#[derive(Debug, Serialize)]
pub struct RuleError {
    pub field: RuleField,
    pub message: &'static str,
}

pub trait RenameRule {
    fn apply(&self, name: &str) -> Result<Option<String>, &'static str>;
}

pub fn build_rule(config: &RuleConfig) -> Result<impl RenameRule + use<'_>, Vec<RuleError>> {
    Ok(ConfiguredRule {
        mode: Mode::Episode(build_episode(config)?),
        extension: None,
    })
}

fn build_episode(config: &RuleConfig) -> Result<EpisodeRule<'_>, Vec<RuleError>> {
    EpisodeRule::build(
        &config.season,
        &config.digits,
        &config.pattern,
        &config.template,
    )
}

fn bounded_number(value: &str, min: usize, max: usize) -> Option<usize> {
    value
        .bytes()
        .all(|digit| digit.is_ascii_digit())
        .then(|| value.parse().ok())
        .flatten()
        .filter(|number| (min..=max).contains(number))
}

fn errors(fields: impl IntoIterator<Item = (RuleField, Option<&'static str>)>) -> Vec<RuleError> {
    fields
        .into_iter()
        .filter_map(|(field, message)| message.map(|message| RuleError { field, message }))
        .collect()
}

enum Mode<'a> {
    Episode(EpisodeRule<'a>),
    Regex(RegexRule<'a>),
    Numbering(NumberingRule<'a>),
    Text(TextRule<'a>),
}

pub struct ConfiguredRule<'a> {
    mode: Mode<'a>,
    extension: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Candidate {
    NoMatch,
    Unchanged,
    Proposed(String),
}

pub const LONG_NAME: &str = "El nombre supera el límite de componente o ruta de Windows.";

impl EditRule {
    pub fn build(&self) -> Result<ConfiguredRule<'_>, Vec<RuleError>> {
        let (mode, extension) = match self {
            Self::Episode { rule, extension } => {
                (build_episode(rule).map(Mode::Episode), extension)
            }
            Self::Regex {
                pattern,
                replacement,
                ignore_case,
                extension,
            } => (
                RegexRule::build(pattern, replacement, *ignore_case)
                    .map(Mode::Regex)
                    .map_err(|message| {
                        vec![RuleError {
                            field: RuleField::Pattern,
                            message,
                        }]
                    }),
                extension,
            ),
            Self::Numbering {
                start,
                step,
                width,
                template,
                extension,
            } => (
                NumberingRule::build(start, step, width, template).map(Mode::Numbering),
                extension,
            ),
            Self::Text {
                search,
                replacement,
                prefix,
                suffix,
                capitalization,
                extension,
            } => (
                Ok(Mode::Text(TextRule {
                    search,
                    replacement,
                    prefix,
                    suffix,
                    capitalization: *capitalization,
                })),
                extension,
            ),
        };
        let extension = match extension {
            Extension::Keep => Ok(None),
            Extension::Remove => Ok(Some(String::new())),
            Extension::Change { value } => {
                let component = value.strip_prefix('.').unwrap_or(value);
                if component.is_empty()
                    || component.chars().any(|character| {
                        character == '.'
                            || character.is_whitespace()
                            || file_name::invalid_character(character)
                    })
                {
                    Err(RuleError {
                        field: RuleField::Extension,
                        message: "Introduce una extensión simple, sin espacios ni puntos adicionales.",
                    })
                } else {
                    Ok(Some(format!(".{component}")))
                }
            }
        };
        match (mode, extension) {
            (Ok(mode), Ok(extension)) => Ok(ConfiguredRule { mode, extension }),
            (mode, extension) => {
                let mut failures = mode.err().unwrap_or_default();
                failures.extend(extension.err());
                Err(failures)
            }
        }
    }
}

impl ConfiguredRule<'_> {
    pub fn edit_extension<'a>(&'a self, original: &'a str, manual: bool) -> &'a str {
        let (stem, original_extension) = file_name::split(original);
        let no_match = manual
            && match &self.mode {
                Mode::Episode(rule) => rule.episode_number(stem).is_none(),
                Mode::Regex(rule) => !rule.matches(stem),
                _ => false,
            };
        if no_match {
            original_extension
        } else {
            self.extension.as_deref().unwrap_or(original_extension)
        }
    }

    pub fn calculate_manual(
        &self,
        original: &str,
        base: &str,
        extension: &str,
        budget: Result<usize, &'static str>,
    ) -> Result<Candidate, &'static str> {
        let mut output = Output::new(original, budget);
        output.push(base)?;
        output.finish_name(extension)
    }

    pub fn uses_date(&self) -> bool {
        match &self.mode {
            Mode::Episode(rule) => rule.uses_date(),
            Mode::Numbering(rule) => rule.uses_date(),
            _ => false,
        }
    }

    pub fn calculate(
        &self,
        original: &str,
        position: usize,
        date: Option<&str>,
        budget: Result<usize, &'static str>,
    ) -> Result<Candidate, &'static str> {
        let (stem, original_extension) = file_name::split(original);
        let mut output = Output::new(original, budget);
        let emit = |part: &str| output.push(part);
        match &self.mode {
            Mode::Episode(rule) => {
                if !rule.emit(stem, date, emit)? {
                    return Ok(Candidate::NoMatch);
                }
            }
            Mode::Regex(rule) => {
                if !rule.emit(stem, emit)? {
                    return Ok(Candidate::NoMatch);
                }
            }
            Mode::Numbering(rule) => rule.emit(stem, position, date, emit)?,
            Mode::Text(rule) => {
                let ceiling = original.encode_utf16().count().max(budget.unwrap_or(0));
                rule.emit(stem, ceiling, budget.err().unwrap_or(LONG_NAME), emit)?;
            }
        }
        output.finish_name(self.extension.as_deref().unwrap_or(original_extension))
    }
}

impl RenameRule for ConfiguredRule<'_> {
    fn apply(&self, name: &str) -> Result<Option<String>, &'static str> {
        Ok(match self.calculate(name, 0, None, Ok(usize::MAX))? {
            Candidate::NoMatch | Candidate::Unchanged => None,
            Candidate::Proposed(name) => Some(name),
        })
    }
}

struct Output<'a> {
    original: &'a str,
    matched: usize,
    changed: Option<String>,
    units: usize,
    budget: Result<usize, &'static str>,
}

impl<'a> Output<'a> {
    fn new(original: &'a str, budget: Result<usize, &'static str>) -> Self {
        Self {
            original,
            matched: 0,
            changed: None,
            units: 0,
            budget,
        }
    }

    fn finish_name(mut self, extension: &str) -> Result<Candidate, &'static str> {
        if self.matched == 0 && self.units == 0 {
            return Err("El nombre antes de la extensión está vacío.");
        }
        self.push(extension)?;
        self.finish()
    }

    fn push(&mut self, part: &str) -> Result<(), &'static str> {
        if self.changed.is_none() && self.original[self.matched..].starts_with(part) {
            self.matched += part.len();
            return Ok(());
        }
        let maximum = self.budget?;
        if self.changed.is_none() {
            let prefix = &self.original[..self.matched];
            self.units = prefix.encode_utf16().count();
            if self.units > maximum {
                return Err(LONG_NAME);
            }
            self.changed = Some(prefix.to_owned());
        }
        let remaining = maximum - self.units;
        let units = part
            .encode_utf16()
            .take(remaining.saturating_add(1))
            .count();
        if units > remaining {
            return Err(LONG_NAME);
        }
        self.units += units;
        self.changed
            .as_mut()
            .expect("El prefijo ya se registró")
            .push_str(part);
        Ok(())
    }

    fn finish(self) -> Result<Candidate, &'static str> {
        let name = match self.changed {
            Some(name) => name,
            None if self.matched == self.original.len() => return Ok(Candidate::Unchanged),
            None => {
                let name = &self.original[..self.matched];
                if name.encode_utf16().count() > self.budget? {
                    return Err(LONG_NAME);
                }
                name.to_owned()
            }
        };
        file_name::validate(&name)?;
        Ok(Candidate::Proposed(name))
    }
}
