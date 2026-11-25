use super::{
    RuleError, RuleField, bounded_number, errors,
    template::{Template, Values},
};
use regex::Regex;

pub struct EpisodeRule<'a> {
    pattern: Regex,
    season: String,
    digits: usize,
    template: Template<'a>,
}

impl<'a> EpisodeRule<'a> {
    pub fn build(
        season: &str,
        digits: &str,
        pattern: &str,
        template: &'a str,
    ) -> Result<Self, Vec<RuleError>> {
        let season = bounded_number(season, 0, 999).ok_or("Introduce una temporada de 0 a 999.");
        let digits = bounded_number(digits, 1, 9).ok_or("Introduce un ancho de 1 a 9.");
        let pattern = Regex::new(pattern)
            .map_err(|_| "Patrón no válido. Revisa la sintaxis. No se admiten anticipaciones, retrospecciones ni referencias a grupos dentro del patrón.")
            .and_then(|pattern| {
                if pattern.captures_len() > 1 {
                    Ok(pattern)
                } else {
                    Err("Incluye un grupo de captura para el número de episodio.")
                }
            });
        let template = Template::parse(template, false);
        match (season, digits, pattern, template) {
            (Ok(season), Ok(digits), Ok(pattern), Ok(template)) => Ok(Self {
                pattern,
                season: format!("{season:0digits$}"),
                digits,
                template,
            }),
            (season, digits, pattern, template) => Err(errors([
                (RuleField::Season, season.err()),
                (RuleField::Digits, digits.err()),
                (RuleField::Pattern, pattern.err()),
                (RuleField::Template, template.err()),
            ])),
        }
    }

    pub fn uses_date(&self) -> bool {
        self.template.uses_date()
    }

    pub fn episode_number(&self, stem: &str) -> Option<usize> {
        let episode = self.pattern.captures(stem)?.get(1)?.as_str();
        bounded_number(episode, 0, 999_999_999).filter(|_| episode.len() <= 9)
    }

    pub fn emit(
        &self,
        stem: &str,
        date: Option<&str>,
        push: impl FnMut(&str) -> Result<(), &'static str>,
    ) -> Result<bool, &'static str> {
        let Some(number) = self.episode_number(stem) else {
            return Ok(false);
        };
        let episode = format!("{number:0width$}", width = self.digits);
        self.template.emit(
            Values {
                name: stem,
                season: &self.season,
                episode: &episode,
                date,
                ..Values::default()
            },
            push,
        )?;
        Ok(true)
    }
}
