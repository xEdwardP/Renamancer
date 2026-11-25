use regex::{Regex, RegexBuilder};

enum Piece<'a> {
    Literal(&'a str),
    Capture(usize),
}

pub struct RegexRule<'a> {
    pattern: Regex,
    replacement: Vec<Piece<'a>>,
}

impl<'a> RegexRule<'a> {
    pub fn matches(&self, stem: &str) -> bool {
        self.pattern.is_match(stem)
    }

    pub fn build(
        pattern: &str,
        replacement: &'a str,
        ignore_case: bool,
    ) -> Result<Self, &'static str> {
        if pattern.is_empty() {
            return Err("Introduce un patrón.");
        }
        let pattern = RegexBuilder::new(pattern)
            .case_insensitive(ignore_case)
            .build()
            .map_err(|_| "Patrón no válido. Revisa la sintaxis de la expresión regular.")?;
        let replacement = parse_replacement(replacement, &pattern);
        Ok(Self {
            pattern,
            replacement,
        })
    }

    pub fn emit(
        &self,
        stem: &str,
        mut push: impl FnMut(&str) -> Result<(), &'static str>,
    ) -> Result<bool, &'static str> {
        let mut matches = self.pattern.captures_iter(stem).peekable();
        if matches.peek().is_none() {
            return Ok(false);
        }
        let mut end = 0;
        for captures in matches {
            let hit = captures
                .get(0)
                .expect("La coincidencia completa siempre participa");
            push(&stem[end..hit.start()])?;
            for piece in &self.replacement {
                match piece {
                    Piece::Literal(text) => push(text)?,
                    Piece::Capture(index) => {
                        if let Some(capture) = captures.get(*index) {
                            push(capture.as_str())?;
                        }
                    }
                }
            }
            end = hit.end();
        }
        push(&stem[end..])?;
        Ok(true)
    }
}

fn parse_replacement<'a>(mut text: &'a str, pattern: &Regex) -> Vec<Piece<'a>> {
    let mut pieces = Vec::new();
    let mut closing_brace_exists = true;
    while let Some((literal, tail)) = text.split_once('$') {
        pieces.push(Piece::Literal(literal));
        if let Some(rest) = tail.strip_prefix('$') {
            pieces.push(Piece::Literal("$"));
            text = rest;
            continue;
        }
        let reference = if let Some(braced) = tail.strip_prefix('{') {
            let split = if closing_brace_exists {
                braced.split_once('}')
            } else {
                None
            };
            closing_brace_exists = split.is_some();
            split
        } else {
            let size = tail
                .bytes()
                .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                .count();
            (size != 0).then(|| tail.split_at(size))
        };
        if let Some((key, rest)) = reference {
            let index = key
                .parse::<usize>()
                .ok()
                .or_else(|| pattern.capture_names().position(|name| name == Some(key)));
            if let Some(index) = index {
                pieces.push(Piece::Capture(index));
            }
            text = rest;
        } else {
            pieces.push(Piece::Literal("$"));
            text = tail;
        }
    }
    pieces.push(Piece::Literal(text));
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_fragment_stops_expansion_before_later_captures() {
        let rule = RegexRule::build("(a)", "$1$1$1$1$1", false).unwrap();
        let mut visited = 0;
        let result = rule.emit("a", |part| {
            visited += part.len();
            if visited > 2 { Err("límite") } else { Ok(()) }
        });
        assert_eq!(result, Err("límite"));
        assert_eq!(visited, 3);
    }
}
