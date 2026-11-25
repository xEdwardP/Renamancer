enum Part<'a> {
    Literal(&'a str),
    Name,
    Season,
    Episode,
    Number(Option<usize>),
    Date,
}

pub struct Template<'a>(Vec<Part<'a>>);

#[derive(Default)]
pub struct Values<'a> {
    pub name: &'a str,
    pub season: &'a str,
    pub episode: &'a str,
    pub number: u64,
    pub width: usize,
    pub date: Option<&'a str>,
}

impl<'a> Template<'a> {
    pub fn parse(mut input: &'a str, numbering: bool) -> Result<Self, &'static str> {
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
                "fecha" => parts.push(Part::Date),
                "temporada" if !numbering => parts.push(Part::Season),
                "episodio" if !numbering => parts.push(Part::Episode),
                "n" if numbering => parts.push(Part::Number(None)),
                "ext" if rest.is_empty() => {}
                "ext" => return Err("{ext} solo puede aparecer una vez y al final."),
                key if numbering => match key.as_bytes() {
                    [b'n', b':', b'0', width @ b'1'..=b'9'] => {
                        parts.push(Part::Number(Some(usize::from(width - b'0'))))
                    }
                    _ => return Err("Usa {nombre}, {n}, {n:01} a {n:09}, {fecha} y {ext}."),
                },
                _ => return Err("Usa {nombre}, {temporada}, {episodio}, {fecha} y {ext}."),
            }
            input = rest;
        }
        parts.push(Part::Literal(input));
        if numbering && !parts.iter().any(|part| matches!(part, Part::Number(_))) {
            return Err("Incluye {n} o un contador con relleno en la plantilla.");
        }
        Ok(Self(parts))
    }

    pub fn uses_date(&self) -> bool {
        self.0.iter().any(|part| matches!(part, Part::Date))
    }

    pub fn emit(
        &self,
        values: Values<'_>,
        mut push: impl FnMut(&str) -> Result<(), &'static str>,
    ) -> Result<(), &'static str> {
        for part in &self.0 {
            match part {
                Part::Literal(text) => push(text)?,
                Part::Name => push(values.name)?,
                Part::Season => push(values.season)?,
                Part::Episode => push(values.episode)?,
                Part::Date => push(
                    values
                        .date
                        .ok_or("No se pudo obtener la fecha local de modificación.")?,
                )?,
                Part::Number(width) => {
                    let width = width.unwrap_or(values.width);
                    push(&format!("{:0width$}", values.number))?;
                }
            }
        }
        Ok(())
    }
}
