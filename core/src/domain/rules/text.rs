use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capitalization {
    Keep,
    Upper,
    Lower,
}

pub struct TextRule<'a> {
    pub search: &'a str,
    pub replacement: &'a str,
    pub prefix: &'a str,
    pub suffix: &'a str,
    pub capitalization: Capitalization,
}

impl TextRule<'_> {
    pub fn emit(
        &self,
        stem: &str,
        ceiling: usize,
        limit_error: &'static str,
        mut push: impl FnMut(&str) -> Result<(), &'static str>,
    ) -> Result<(), &'static str> {
        if matches!(self.capitalization, Capitalization::Keep) {
            return self.fragments(stem, push);
        }

        // Default Unicode case mapping emits at least one scalar per input scalar.
        // Bound the intermediate base while preserving the context of final sigma.
        let mut remaining = ceiling;
        let mut base = String::new();
        self.fragments(stem, |part| {
            let count = part.chars().take(remaining.saturating_add(1)).count();
            remaining = remaining.checked_sub(count).ok_or(limit_error)?;
            base.push_str(part);
            Ok(())
        })?;
        match self.capitalization {
            Capitalization::Upper => push(&base.to_uppercase()),
            Capitalization::Lower => push(&base.to_lowercase()),
            Capitalization::Keep => unreachable!(),
        }
    }

    fn fragments(
        &self,
        stem: &str,
        mut push: impl FnMut(&str) -> Result<(), &'static str>,
    ) -> Result<(), &'static str> {
        push(self.prefix)?;
        let mut end = 0;
        if !self.search.is_empty() {
            for (start, matched) in stem.match_indices(self.search) {
                push(&stem[end..start])?;
                push(self.replacement)?;
                end = start + matched.len();
            }
        }
        push(&stem[end..])?;
        push(self.suffix)
    }
}
