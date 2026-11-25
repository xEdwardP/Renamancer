pub mod folder;

use crate::domain::{
    file_name,
    order::Sort,
    plan::{self, Check, Outcome},
    rules::{Candidate, EditRule, RenameRule, RuleConfig, RuleError, build_rule},
};
use folder::{
    Folder, FolderAction, Occupancy, OriginFailure, ReadFailure, UNSUPPORTED_PROFILE,
    UNVERIFIED_DESTINATION, UNVERIFIED_PROFILE,
};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{MapAccess, Visitor, value::MapAccessDeserializer},
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

#[derive(Debug, Serialize)]
pub struct FileEntry {
    #[serde(skip)]
    pub path: PathBuf,
    pub id: String,
    pub name: String,
    pub size: u64,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum PreviewRow {
    Proposed { name: String },
    Unchanged,
    Error { reason: &'static str },
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum Preview {
    Ready { rows: Vec<PreviewRow> },
    Invalid { errors: Vec<RuleError> },
    Stale,
}

pub fn generate_preview(files: &[FileEntry], config: &RuleConfig) -> Preview {
    match build_rule(config) {
        Ok(rule) => Preview::Ready {
            rows: files
                .iter()
                .map(|file| match rule.apply(&file.name) {
                    Ok(Some(name)) => PreviewRow::Proposed { name },
                    Ok(None) => PreviewRow::Unchanged,
                    Err(reason) => PreviewRow::Error { reason },
                })
                .collect(),
        },
        Err(errors) => Preview::Invalid { errors },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub selection_revision: u32,
    pub edit_revision: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
    #[serde(flatten)]
    pub revision: Revision,
    pub config: EditRule,
    pub sort: Sort,
    pub excluded_ids: Vec<String>,
    #[serde(default)]
    pub manual_names: Vec<ManualName>,
    pub draft_file_id: Option<String>,
}

#[derive(Debug)]
pub struct ManualName {
    pub file_id: String,
    pub base: String,
}

impl<'de> Deserialize<'de> for ManualName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Fields {
            file_id: String,
            base: String,
        }
        struct ManualNameVisitor;
        impl<'de> Visitor<'de> for ManualNameVisitor {
            type Value = ManualName;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("un objeto con fileId y base")
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                let Fields { file_id, base } =
                    Fields::deserialize(MapAccessDeserializer::new(map))?;
                Ok(ManualName { file_id, base })
            }
        }
        deserializer.deserialize_map(ManualNameVisitor)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Ready,
    Unchanged,
    Excluded,
    Conflict,
    Error,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub file_id: String,
    pub status: Option<Status>,
    pub proposed_name: Option<String>,
    pub reason: Option<String>,
    pub name_edit: Option<NameEdit>,
    pub retryable: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum BaseSource {
    Manual,
    Automatic,
    NoMatch,
    GenerationError,
    ProfileUnavailable,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameEdit {
    pub base: String,
    pub extension: String,
    source: BaseSource,
    pub can_save: bool,
    pub save_reason: Option<String>,
}

impl Row {
    fn set(&mut self, status: Status, reason: Option<&str>) {
        self.status = Some(status);
        self.reason = reason.map(str::to_owned);
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum EditPreview {
    Ready {
        rows: Vec<Row>,
    },
    Invalid {
        rows: Vec<Row>,
        errors: Vec<RuleError>,
    },
    FolderFailure {
        rows: Vec<Row>,
        action: FolderAction,
        reason: &'static str,
    },
    Stale,
}

pub fn calculate_preview(
    files: &[FileEntry],
    request: &PreviewRequest,
    folder: &mut impl Folder,
    current: &dyn Fn() -> bool,
) -> EditPreview {
    if !current() {
        return EditPreview::Stale;
    }
    let excluded = request
        .excluded_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut files = files.iter().collect::<Vec<_>>();
    files.sort_by(|left, right| {
        request.sort.compare(
            (&left.name, &left.kind, left.size),
            (&right.name, &right.kind, right.size),
        )
    });
    let mut rows = files
        .iter()
        .map(|file| Row {
            file_id: file.id.clone(),
            status: excluded
                .contains(file.id.as_str())
                .then_some(Status::Excluded),
            proposed_name: None,
            reason: None,
            name_edit: None,
            retryable: false,
        })
        .collect::<Vec<_>>();
    let rule = match request.config.build() {
        Ok(rule) => rule,
        Err(errors) => return EditPreview::Invalid { rows, errors },
    };
    let included = files
        .iter()
        .copied()
        .filter(|file| !excluded.contains(file.id.as_str()))
        .collect::<Vec<_>>();
    if included.is_empty() {
        return EditPreview::Ready { rows };
    }
    let inventory = match folder.read(&included, rule.uses_date(), current) {
        Ok(inventory) => inventory,
        Err(ReadFailure::Stale) => return EditPreview::Stale,
        Err(ReadFailure::Folder(action)) => return folder_failure(rows, action),
    };
    let budget = inventory.profile.and_then(|profile| profile.budget());
    let manual_names = request
        .manual_names
        .iter()
        .map(|name| (name.file_id.as_str(), name.base.as_str()))
        .collect::<HashMap<_, _>>();
    let mut candidates = (0..rows.len()).map(|_| None).collect::<Vec<_>>();
    for (position, (index, row)) in rows
        .iter_mut()
        .enumerate()
        .filter(|(_, row)| row.status.is_none())
        .enumerate()
    {
        if !current() {
            return EditPreview::Stale;
        }
        let date = match inventory.origins.get(&row.file_id) {
            Some(Ok(date)) => date.as_deref(),
            failure => {
                let cause = failure
                    .and_then(|result| result.as_ref().err())
                    .copied()
                    .unwrap_or(OriginFailure::ReadFailed);
                row.set(Status::Error, Some(cause.reason()));
                row.retryable = cause == OriginFailure::ReadFailed;
                continue;
            }
        };
        let original = &files[index].name;
        let (original_base, original_extension) = file_name::split(original);
        let manual = manual_names.get(row.file_id.as_str()).copied();
        let mut extension = rule.edit_extension(original, manual.is_some());
        let result = if let Some(base) = manual {
            rule.calculate_manual(original, base, extension, budget)
        } else {
            rule.calculate(original, position, date, budget)
        };
        let (base, source) = if let Some(base) = manual {
            (base, BaseSource::Manual)
        } else {
            match &result {
                Ok(Candidate::NoMatch) => {
                    extension = original_extension;
                    (original_base, BaseSource::NoMatch)
                }
                Ok(Candidate::Unchanged) => (
                    original.strip_suffix(extension).unwrap_or(original_base),
                    BaseSource::Automatic,
                ),
                Ok(Candidate::Proposed(name)) => (
                    name.strip_suffix(extension).unwrap_or(name),
                    BaseSource::Automatic,
                ),
                Err(reason) if matches!(*reason, UNVERIFIED_PROFILE | UNSUPPORTED_PROFILE) => {
                    (original_base, BaseSource::ProfileUnavailable)
                }
                Err(_) => (original_base, BaseSource::GenerationError),
            }
        };
        row.name_edit = Some(NameEdit {
            base: base.to_owned(),
            extension: extension.to_owned(),
            source,
            can_save: false,
            save_reason: None,
        });
        match result {
            Ok(Candidate::NoMatch) => row.set(Status::Unchanged, Some("No coincide")),
            Ok(Candidate::Unchanged) => row.set(Status::Unchanged, Some("El nombre queda igual")),
            Ok(Candidate::Proposed(name)) => candidates[index] = Some(name),
            Err(reason) => row.set(Status::Error, Some(reason)),
        }
    }
    let Some(classified) = plan::classify(&candidates, current) else {
        return EditPreview::Stale;
    };
    let ids = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.file_id.clone(), index))
        .collect::<HashMap<_, _>>();
    let draft = request
        .draft_file_id
        .as_ref()
        .and_then(|id| ids.get(id))
        .copied();
    let mut supplemental = None;
    let mut checks = Vec::with_capacity(rows.len());
    let mut invalidated = Vec::new();
    for (index, (name, result)) in candidates.iter().zip(classified).enumerate() {
        if !current() {
            return EditPreview::Stale;
        }
        let needs_supplement = draft == Some(index) && result == Some(Outcome::UncertainNames);
        let lookup = if let Some(name) = name
            .as_ref()
            .filter(|_| result.is_none() || needs_supplement)
        {
            let hit = folder.lookup(name, files[index]);
            invalidated.extend(hit.invalidated_source);
            rows[index].retryable = matches!(hit.occupancy, Occupancy::ReadFailed);
            Some(occupancy_check(hit.occupancy, index, &rows, &files, &ids))
        } else {
            None
        };
        if needs_supplement {
            supplemental = lookup.clone();
        }
        let check = match result {
            Some(outcome) => Check::Terminal(outcome),
            None => lookup.unwrap_or_else(|| {
                if rows[index].retryable
                    || rows[index]
                        .reason
                        .as_deref()
                        .is_some_and(|r| r == UNVERIFIED_PROFILE || r == UNSUPPORTED_PROFILE)
                {
                    Check::Unverified(rows[index].reason.clone().unwrap_or_default())
                } else {
                    Check::Stays
                }
            }),
        };
        checks.push(check);
    }

    for (id, cause) in invalidated {
        if let Some(&index) = ids.get(&id)
            && rows[index].status != Some(Status::Excluded)
        {
            let row = &mut rows[index];
            row.name_edit = None;
            row.retryable = cause == OriginFailure::ReadFailed;
            checks[index] = if row.retryable {
                Check::Unverified(cause.reason().into())
            } else {
                Check::Terminal(Outcome::Error(cause.reason().into()))
            };
        }
    }
    if !current() {
        return EditPreview::Stale;
    }
    if let Err(action) = folder.verify() {
        return folder_failure(rows, action);
    }
    let Some(decisions) = plan::resolve(&checks, current) else {
        return EditPreview::Stale;
    };
    let draft_has_unicode_error =
        draft.is_some_and(|index| decisions[index] == Some(Outcome::UncertainNames));
    let mut blocked_save = None;
    if let (Some(index), Some(check)) = (draft, supplemental)
        && rows[index].name_edit.is_some()
    {
        checks[index] = check;
        let Some(hypothetical) = plan::resolve(&checks, current) else {
            return EditPreview::Stale;
        };
        blocked_save = match &hypothetical[index] {
            Some(Outcome::Conflict(_))
                if matches!(checks[index], Check::Dependency {
                row: target, alias: true, ..
            } if !plan::alias_holder_is_verified(&checks, &hypothetical, target)) =>
            {
                Some(plan::UNCERTAIN_ALIAS.to_owned())
            }
            Some(Outcome::Ready | Outcome::Conflict(_)) => None,
            Some(Outcome::Error(reason)) => Some(reason.clone()),
            _ => Some(UNVERIFIED_DESTINATION.into()),
        };
    }
    for ((row, candidate), decision) in rows.iter_mut().zip(candidates).zip(decisions) {
        match decision {
            Some(Outcome::Ready) => {
                row.set(Status::Ready, None);
                row.proposed_name = candidate;
            }
            Some(Outcome::Conflict(reason)) => {
                row.set(Status::Conflict, Some(&reason));
                row.proposed_name = candidate;
            }
            Some(Outcome::Error(reason)) => row.set(Status::Error, Some(&reason)),
            Some(Outcome::UncertainNames) => row.set(Status::Error, Some(plan::UNCERTAIN_NAMES)),
            None => {}
        }
    }
    if let Some(index) = draft {
        let row = &mut rows[index];
        if let Some(edit) = &mut row.name_edit {
            edit.can_save = matches!(
                row.status,
                Some(Status::Ready | Status::Unchanged | Status::Conflict)
            ) || (draft_has_unicode_error && blocked_save.is_none());
            edit.save_reason = blocked_save;
        }
    }
    EditPreview::Ready { rows }
}

fn occupancy_check(
    occupancy: Occupancy,
    index: usize,
    rows: &[Row],
    files: &[&FileEntry],
    ids: &HashMap<String, usize>,
) -> Check {
    match occupancy {
        Occupancy::Free => Check::Terminal(Outcome::Ready),
        Occupancy::Immovable { directory } => Check::Terminal(Outcome::Conflict(
            if directory {
                "Una carpeta ocupa el destino."
            } else {
                "Un archivo ocupa el destino."
            }
            .into(),
        )),
        Occupancy::ReadFailed => Check::Unverified(UNVERIFIED_DESTINATION.into()),
        Occupancy::Unknown(reason) => Check::Unverified(reason.into()),
        Occupancy::Selected {
            id,
            alias,
            case_only,
        } if id == rows[index].file_id => {
            if case_only {
                Check::Terminal(Outcome::Ready)
            } else if alias {
                Check::Terminal(Outcome::Error(
                    "El destino es un alias corto del propio archivo.".into(),
                ))
            } else {
                Check::Unverified(plan::UNCERTAIN_CASE.into())
            }
        }
        Occupancy::Selected { id, alias, .. } => match ids.get(&id).copied() {
            Some(target) => Check::Dependency {
                row: target,
                alias,
                blocked_reason: if rows[target].status == Some(Status::Excluded) {
                    format!(
                        "«{}» sigue ocupado porque está excluido.",
                        files[target].name
                    )
                } else {
                    format!("«{}» conserva su nombre.", files[target].name)
                },
            },
            None => Check::Unverified(UNVERIFIED_DESTINATION.into()),
        },
    }
}

fn folder_failure(mut rows: Vec<Row>, action: FolderAction) -> EditPreview {
    let reason = match action {
        FolderAction::Retry => "No se pudo leer la carpeta.",
        FolderAction::Reselect => {
            "No se pudo verificar la carpeta. Selecciona los archivos de nuevo."
        }
    };
    for row in &mut rows {
        if row.status != Some(Status::Excluded) {
            row.set(Status::Error, Some(reason));
            row.name_edit = None;
            row.retryable = false;
        }
    }
    EditPreview::FolderFailure {
        rows,
        action,
        reason,
    }
}
