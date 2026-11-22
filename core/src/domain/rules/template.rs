enum Part<'a> {
    Literal(&'a str),
    Name,
    Season,
    Episode,
}

pub struct Template<'a>(Vec<Part<'a>>);

impl<'a> Template<'a> {
    pub fn parse(mut input: &'a str) -> Result<Self, &'static str> {
        let mut parts = Vec::new();
        while let Some(index) = input.find(['{', '}']) {
            let (literal, variable) = input.split_at(index);
            let Some((key, rest)) = variable
                .strip_prefix('{')
                .and_then(|tail| tail.split_once('}'))
            else {
                return Err("Revisa las llaves de la plantilla.");
            };
            if key.contains('{') {
                return Err("Revisa las llaves de la plantilla.");
            }
            parts.push(Part::Literal(literal));
            match key {
                "nombre" => parts.push(Part::Name),
                "temporada" => parts.push(Part::Season),
                "episodio" => parts.push(Part::Episode),
                "ext" if rest.is_empty() => {}
                "ext" => return Err("{ext} solo puede aparecer una vez y al final."),
                _ => return Err("Usa solo {nombre}, {temporada}, {episodio} y {ext}."),
            }
            input = rest;
        }
        parts.push(Part::Literal(input));
        Ok(Self(parts))
    }

    pub fn render(&self, name: &str, season: &str, episode: &str) -> String {
        self.0
            .iter()
            .map(|part| match part {
                Part::Literal(text) => text,
                Part::Name => name,
                Part::Season => season,
                Part::Episode => episode,
            })
            .collect()
    }
}
