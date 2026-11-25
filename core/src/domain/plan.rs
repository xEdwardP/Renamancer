use std::collections::HashMap;

pub const UNCERTAIN_NAMES: &str =
    "No se pudo verificar que los destinos propuestos sean distintos.";
pub const UNCERTAIN_ALIAS: &str = "No se pudo verificar que se libere el alias corto del destino.";
pub const UNCERTAIN_CASE: &str = "No se pudo verificar el cambio de capitalización.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Ready,
    Conflict(String),
    Error(String),
    UncertainNames,
}

#[derive(Clone, Debug)]
pub enum Check {
    Terminal(Outcome),
    Unverified(String),
    Dependency {
        row: usize,
        alias: bool,
        blocked_reason: String,
    },
    Stays,
}

pub fn classify(
    candidates: &[Option<String>],
    current: &dyn Fn() -> bool,
) -> Option<Vec<Option<Outcome>>> {
    let mut indexed = HashMap::<String, Vec<usize>>::new();
    for (index, name) in candidates.iter().enumerate() {
        if let Some(name) = name {
            indexed
                .entry(name.to_ascii_lowercase())
                .or_default()
                .push(index);
        }
    }

    let mut result = vec![None; candidates.len()];
    let mut by_length = HashMap::<usize, Vec<(String, Vec<usize>)>>::new();
    for (key, members) in indexed {
        if members.len() > 1 {
            for &index in &members {
                result[index] = Some(Outcome::Conflict("Destino duplicado.".into()));
            }
        }
        by_length
            .entry(key.encode_utf16().count())
            .or_default()
            .push((key, members));
    }

    for groups in by_length.values_mut() {
        groups.sort_unstable_by_key(|(key, _)| key.is_ascii());
        for (offset, (left, left_rows)) in groups
            .iter()
            .enumerate()
            .take_while(|(_, (key, _))| !key.is_ascii())
        {
            for (right, right_rows) in &groups[offset + 1..] {
                if !current() {
                    return None;
                }
                let distinct = left
                    .encode_utf16()
                    .zip(right.encode_utf16())
                    .any(|(a, b)| a <= 127 && b <= 127 && a != b);
                if !distinct {
                    for &index in left_rows.iter().chain(right_rows) {
                        if result[index].is_none() {
                            result[index] = Some(Outcome::UncertainNames);
                        }
                    }
                }
            }
        }
    }
    Some(result)
}

pub fn resolve(checks: &[Check], current: &dyn Fn() -> bool) -> Option<Vec<Option<Outcome>>> {
    let mut resolved = vec![false; checks.len()];
    let mut decisions = vec![None; checks.len()];
    let mut positions = vec![None; checks.len()];
    let mut path: Vec<usize> = Vec::new();

    for start in 0..checks.len() {
        let mut row = start;
        while !resolved[row] {
            if !current() {
                return None;
            }
            if let Some(cycle_start) = positions[row] {
                let cycle = &path[cycle_start..];
                let has_alias = cycle
                    .iter()
                    .any(|&index| matches!(checks[index], Check::Dependency { alias: true, .. }));
                for &index in cycle {
                    let Check::Dependency {
                        alias,
                        blocked_reason,
                        ..
                    } = &checks[index]
                    else {
                        unreachable!()
                    };
                    decisions[index] = Some(if !has_alias {
                        Outcome::Ready
                    } else if *alias {
                        Outcome::Error(UNCERTAIN_ALIAS.into())
                    } else {
                        Outcome::Conflict(blocked_reason.clone())
                    });
                    resolved[index] = true;
                }
                break;
            }
            match &checks[row] {
                Check::Terminal(outcome) => {
                    decisions[row] = Some(outcome.clone());
                    resolved[row] = true;
                }
                Check::Unverified(reason) => {
                    decisions[row] = Some(Outcome::Error(reason.clone()));
                    resolved[row] = true;
                }
                Check::Stays => resolved[row] = true,
                Check::Dependency { row: target, .. } => {
                    positions[row] = Some(path.len());
                    path.push(row);
                    row = *target;
                }
            }
        }
        while let Some(index) = path.pop() {
            positions[index] = None;
            if !resolved[index] {
                let Check::Dependency {
                    row: target,
                    alias,
                    blocked_reason,
                } = &checks[index]
                else {
                    unreachable!()
                };
                decisions[index] = Some(match (&decisions[*target], alias) {
                    (Some(Outcome::Ready), false) => Outcome::Ready,
                    (Some(Outcome::Ready), true) => Outcome::Error(UNCERTAIN_ALIAS.into()),
                    _ => Outcome::Conflict(blocked_reason.clone()),
                });
                resolved[index] = true;
            }
        }
    }
    Some(decisions)
}

pub fn alias_holder_is_verified(
    checks: &[Check],
    decisions: &[Option<Outcome>],
    mut row: usize,
) -> bool {
    for _ in 0..checks.len() {
        match (&checks[row], &decisions[row]) {
            (Check::Unverified(_) | Check::Terminal(Outcome::UncertainNames), _) => return false,
            (Check::Dependency { .. }, Some(Outcome::Error(_))) => return false,
            (Check::Dependency { row: target, .. }, _) => row = *target,
            _ => return true,
        }
    }
    false
}
