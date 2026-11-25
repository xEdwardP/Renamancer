use serde::Deserialize;
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Column {
    #[default]
    Name,
    Type,
    Size,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    #[default]
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct Sort {
    pub column: Column,
    pub direction: Direction,
}

pub fn natural_name(left: &str, right: &str) -> Ordering {
    natord::compare_ignore_case(left, right).then_with(|| left.cmp(right))
}

impl Sort {
    pub fn compare(self, left: (&str, &str, u64), right: (&str, &str, u64)) -> Ordering {
        let (left_name, left_type, left_size) = left;
        let (right_name, right_type, right_size) = right;
        let primary = match self.column {
            Column::Name => natord::compare_ignore_case(left_name, right_name),
            Column::Type => left_type.cmp(right_type),
            Column::Size => left_size.cmp(&right_size),
        };
        match self.direction {
            Direction::Asc => primary,
            Direction::Desc => primary.reverse(),
        }
        .then_with(|| natural_name(left_name, right_name))
    }
}
