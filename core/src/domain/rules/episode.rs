use super::{RenameRule, RuleConfig, RuleError, RuleField, template::Template};
use crate::domain::file_name;
use regex::Regex;

pub struct EpisodeRule<'a> {
    pattern: Regex,
    season: String,
    digits: usize,
    template: Template<'a>,
}

impl<'a> EpisodeRule<'a> {
    pub fn build(config: &'a RuleConfig) -> Result<Self, Vec<RuleError>> {
        let season =
            bounded_number(&config.season, 0, 999).ok_or("Introduce una temporada de 0 a 999.");
        let digits = bounded_number(&config.digits, 1, 9).ok_or("Introduce un ancho de 1 a 9.");
        let pattern = Regex::new(&config.pattern)
            .map_err(|_| "Patrón no válido. Revisa la sintaxis. No se admiten anticipaciones, retrospecciones ni referencias a grupos dentro del patrón.")
            .and_then(|pattern| {
                if pattern.captures_len() > 1 {
                    Ok(pattern)
                } else {
                    Err("Incluye un grupo de captura para el número de episodio.")
                }
            });
        let template = Template::parse(&config.template);
        match (season, digits, pattern, template) {
            (Ok(season), Ok(digits), Ok(pattern), Ok(template)) => Ok(Self {
                pattern,
                season: format!("{season:0digits$}"),
                digits,
                template,
            }),
            (season, digits, pattern, template) => Err([
                (RuleField::Season, season.err()),
                (RuleField::Digits, digits.err()),
                (RuleField::Pattern, pattern.err()),
                (RuleField::Template, template.err()),
            ]
            .into_iter()
            .filter_map(|(field, message)| message.map(|message| RuleError { field, message }))
            .collect()),
        }
    }
}

fn bounded_number(value: &str, min: usize, max: usize) -> Option<usize> {
    if !value.bytes().all(|digit| digit.is_ascii_digit()) {
        return None;
    }
    value
        .parse()
        .ok()
        .filter(|number| (min..=max).contains(number))
}

impl RenameRule for EpisodeRule<'_> {
    fn apply(&self, name: &str) -> Result<Option<String>, &'static str> {
        let (stem, extension) = file_name::split(name);
        let Some(episode) = self
            .pattern
            .captures(stem)
            .and_then(|matched| matched.get(1))
        else {
            return Ok(None);
        };
        let episode = episode.as_str();
        let Some(number) = bounded_number(episode, 0, 999_999_999).filter(|_| episode.len() <= 9)
        else {
            return Ok(None);
        };
        let episode = format!("{number:0width$}", width = self.digits);
        let mut candidate = self.template.render(stem, &self.season, &episode);
        if candidate.is_empty() {
            return Err("El nombre antes de la extensión está vacío.");
        }
        candidate.push_str(extension);
        if candidate == name {
            return Ok(None);
        }
        file_name::validate(&candidate)?;
        Ok(Some(candidate))
    }
}
