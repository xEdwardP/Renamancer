use super::{
    RuleError, RuleField, bounded_number, errors,
    template::{Template, Values},
};

const MAX_NUMBER: usize = 999_999_999;

pub struct NumberingRule<'a> {
    start: u64,
    step: u64,
    width: usize,
    template: Template<'a>,
}

impl<'a> NumberingRule<'a> {
    pub fn build(
        start: &str,
        step: &str,
        width: &str,
        template: &'a str,
    ) -> Result<Self, Vec<RuleError>> {
        let start =
            bounded_number(start, 0, MAX_NUMBER).ok_or("Introduce un inicio de 0 a 999999999.");
        let step = bounded_number(step, 1, MAX_NUMBER).ok_or("Introduce un paso de 1 a 999999999.");
        let width = bounded_number(width, 1, 9).ok_or("Introduce un ancho de 1 a 9.");
        let template = Template::parse(template, true);
        match (start, step, width, template) {
            (Ok(start), Ok(step), Ok(width), Ok(template)) => Ok(Self {
                start: start as u64,
                step: step as u64,
                width,
                template,
            }),
            (start, step, width, template) => Err(errors([
                (RuleField::Start, start.err()),
                (RuleField::Step, step.err()),
                (RuleField::Width, width.err()),
                (RuleField::Template, template.err()),
            ])),
        }
    }

    pub fn uses_date(&self) -> bool {
        self.template.uses_date()
    }

    pub fn emit(
        &self,
        stem: &str,
        position: usize,
        date: Option<&str>,
        push: impl FnMut(&str) -> Result<(), &'static str>,
    ) -> Result<(), &'static str> {
        let number = self
            .step
            .checked_mul(position as u64)
            .and_then(|offset| self.start.checked_add(offset))
            .filter(|&number| number <= MAX_NUMBER as u64)
            .ok_or("El número calculado supera 999999999.")?;
        self.template.emit(
            Values {
                name: stem,
                number,
                width: self.width,
                date,
                ..Values::default()
            },
            push,
        )
    }
}
