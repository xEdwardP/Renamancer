use renombrador_core::{
    application::{
        EditPreview, FileEntry, ManualName, PreviewRequest, Revision, Row, Status,
        calculate_preview,
        folder::{
            AMBIGUOUS_ALIAS, Folder, FolderAction, Inventory, Lookup, MISSING_ORIGIN, Occupancy,
            OriginFailure, Profile, ReadFailure, UNSUPPORTED_PROFILE, UNVERIFIED_DESTINATION,
            UNVERIFIED_PROFILE,
        },
    },
    domain::{
        order::{Column, Direction, Sort},
        plan::{UNCERTAIN_ALIAS, UNCERTAIN_NAMES},
        rules::{Candidate, Capitalization, EditRule, Extension, LONG_NAME, RuleConfig, RuleField},
    },
};
use std::{collections::HashMap, path::PathBuf};

struct Adapter {
    hits: HashMap<String, Lookup>,
    consulted: Vec<String>,
    origins: HashMap<String, Result<Option<String>, OriginFailure>>,
    profile: Result<Profile, &'static str>,
    action: Option<FolderAction>,
    finish: Option<FolderAction>,
    reads: usize,
}

impl Default for Adapter {
    fn default() -> Self {
        Self {
            hits: HashMap::new(),
            consulted: Vec::new(),
            origins: HashMap::new(),
            profile: Ok(Profile {
                prefix_units: 20,
                component_limit: 255,
            }),
            action: None,
            finish: None,
            reads: 0,
        }
    }
}

impl Folder for Adapter {
    fn read(
        &mut self,
        included: &[&FileEntry],
        _: bool,
        _: &dyn Fn() -> bool,
    ) -> Result<Inventory, ReadFailure> {
        self.reads += 1;
        if let Some(action) = self.action {
            return Err(ReadFailure::Folder(action));
        }
        Ok(Inventory {
            profile: self.profile,
            origins: included
                .iter()
                .map(|file| {
                    (
                        file.id.clone(),
                        self.origins.get(&file.id).cloned().unwrap_or(Ok(None)),
                    )
                })
                .collect(),
        })
    }

    fn lookup(&mut self, destination: &str, _: &FileEntry) -> Lookup {
        self.consulted.push(destination.into());
        self.hits
            .remove(destination)
            .unwrap_or_else(|| Occupancy::Free.into())
    }

    fn verify(&self) -> Result<(), FolderAction> {
        self.finish.map_or(Ok(()), Err)
    }
}

fn files(names: &[&str]) -> Vec<FileEntry> {
    names
        .iter()
        .enumerate()
        .map(|(index, name)| FileEntry {
            id: format!("f{index}"),
            name: (*name).into(),
            path: PathBuf::new(),
            size: 20,
            kind: "TXT".into(),
        })
        .collect()
}

fn request(config: EditRule) -> PreviewRequest {
    PreviewRequest {
        config,
        sort: Sort::default(),
        excluded_ids: vec![],
        manual_names: vec![],
        draft_file_id: None,
        revision: Revision {
            selection_revision: 1,
            edit_revision: 1,
        },
    }
}

fn numbering(start: &str, template: &str) -> EditRule {
    EditRule::Numbering {
        start: start.into(),
        step: "1".into(),
        width: "1".into(),
        template: template.into(),
        extension: Extension::Keep,
    }
}

fn regex(pattern: &str, replacement: &str, extension: Extension) -> EditRule {
    EditRule::Regex {
        pattern: pattern.into(),
        replacement: replacement.into(),
        ignore_case: false,
        extension,
    }
}

fn text(search: &str, replacement: &str, capitalization: Capitalization) -> EditRule {
    EditRule::Text {
        search: search.into(),
        replacement: replacement.into(),
        prefix: String::new(),
        suffix: String::new(),
        capitalization,
        extension: Extension::Keep,
    }
}

fn selected(id: &str, alias: bool, case_only: bool) -> Lookup {
    Occupancy::Selected {
        id: id.into(),
        alias,
        case_only,
    }
    .into()
}

fn ready(preview: EditPreview) -> Vec<Row> {
    let EditPreview::Ready { rows } = preview else {
        panic!("{preview:?}")
    };
    rows
}

fn manual(file_id: &str, base: &str) -> ManualName {
    ManualName {
        file_id: file_id.into(),
        base: base.into(),
    }
}

#[test]
fn manual_base_uses_the_effective_extension_and_keeps_the_counter_position() {
    let entries = files(&["foto.mp4", "video.mp4"]);
    let mut rule = request(regex(
        "video",
        "clip",
        Extension::Change {
            value: "mkv".into(),
        },
    ));
    rule.manual_names = vec![manual("f0", "vista.v2"), manual("f1", "revisado")];
    rule.draft_file_id = Some("f0".into());
    let rows = ready(calculate_preview(
        &entries,
        &rule,
        &mut Adapter::default(),
        &|| true,
    ));
    assert_eq!(rows[0].proposed_name.as_deref(), Some("vista.v2.mp4"));
    assert_eq!(rows[0].name_edit.as_ref().unwrap().extension, ".mp4");
    assert!(rows[0].name_edit.as_ref().unwrap().can_save);
    assert_eq!(rows[1].proposed_name.as_deref(), Some("revisado.mkv"));
    assert!(!rows[1].name_edit.as_ref().unwrap().can_save);
    rule.config = regex("foto", "imagen", Extension::Remove);
    let rows = ready(calculate_preview(
        &entries,
        &rule,
        &mut Adapter::default(),
        &|| true,
    ));
    assert_eq!(rows[0].proposed_name.as_deref(), Some("vista.v2"));

    let entries = files(&["a.txt", "b.txt", "c.txt"]);
    let mut rule = request(numbering("1", "{n}"));
    rule.manual_names = vec![manual("f1", "revisado")];
    let rows = ready(calculate_preview(
        &entries,
        &rule,
        &mut Adapter::default(),
        &|| true,
    ));
    assert_eq!(rows[1].proposed_name.as_deref(), Some("revisado.txt"));
    assert_eq!(rows[2].proposed_name.as_deref(), Some("3.txt"));
    rule.config = numbering("999999999", "{n}");
    let rows = ready(calculate_preview(
        &entries,
        &rule,
        &mut Adapter::default(),
        &|| true,
    ));
    assert_eq!(rows[1].proposed_name.as_deref(), Some("revisado.txt"));
    assert_eq!(rows[2].status, Some(Status::Error));
    rule.draft_file_id = Some("f1".into());
    for invalid in ["", "CON", "otra/ruta"] {
        rule.manual_names = vec![manual("f1", invalid)];
        let rows = ready(calculate_preview(
            &entries,
            &rule,
            &mut Adapter::default(),
            &|| true,
        ));
        assert_eq!(rows[1].status, Some(Status::Error), "{invalid:?}");
        assert!(!rows[1].name_edit.as_ref().unwrap().can_save);
    }
}

#[test]
fn a_generation_failure_has_an_editable_fallback_and_origin_failure_removes_it() {
    let entries = files(&["video.mp4"]);
    let mut rule = request(numbering("1", "{n}-{fecha}"));
    let EditRule::Numbering { extension, .. } = &mut rule.config else {
        unreachable!()
    };
    *extension = Extension::Change {
        value: "mkv".into(),
    };
    let mut adapter = Adapter::default();
    let rows = ready(calculate_preview(&entries, &rule, &mut adapter, &|| true));
    assert_eq!(rows[0].status, Some(Status::Error));
    let edit = rows[0].name_edit.as_ref().unwrap();
    assert_eq!(edit.base, "video");
    assert_eq!(edit.extension, ".mkv");
    rule.manual_names = vec![manual("f0", "video")];
    rule.draft_file_id = Some("f0".into());
    let rows = ready(calculate_preview(&entries, &rule, &mut adapter, &|| true));
    assert_eq!(rows[0].proposed_name.as_deref(), Some("video.mkv"));
    assert!(rows[0].name_edit.as_ref().unwrap().can_save);
    for (failure, retryable) in [
        (OriginFailure::ReadFailed, true),
        (OriginFailure::Missing, false),
    ] {
        adapter.origins.insert("f0".into(), Err(failure));
        let rows = ready(calculate_preview(&entries, &rule, &mut adapter, &|| true));
        assert_eq!(rows[0].status, Some(Status::Error));
        assert_eq!(rows[0].retryable, retryable);
        assert!(rows[0].name_edit.is_none());
    }
}

#[test]
fn a_unicode_draft_is_independently_checked_without_changing_the_table_decision() {
    let entries = files(&["é1-origen.txt", "ü1-origen.txt"]);
    let mut rule = request(text("-origen", "-nuevo", Capitalization::Keep));
    rule.manual_names = vec![manual("f0", "a1-nuevo")];
    rule.draft_file_id = Some("f0".into());
    let mut adapter = Adapter::default();
    let rows = ready(calculate_preview(&entries, &rule, &mut adapter, &|| true));
    let first = rows.iter().find(|row| row.file_id == "f0").unwrap();
    assert_eq!(adapter.consulted, ["a1-nuevo.txt"]);
    assert_eq!(first.reason.as_deref(), Some(UNCERTAIN_NAMES));
    assert!(first.name_edit.as_ref().unwrap().can_save);
    adapter
        .hits
        .insert("a1-nuevo.txt".into(), Occupancy::ReadFailed.into());
    let rows = ready(calculate_preview(&entries, &rule, &mut adapter, &|| true));
    let first = rows.iter().find(|row| row.file_id == "f0").unwrap();
    assert_eq!(first.reason.as_deref(), Some(UNCERTAIN_NAMES));
    assert!(first.retryable);
    let edit = first.name_edit.as_ref().unwrap();
    assert!(!edit.can_save);
    assert_eq!(edit.save_reason.as_deref(), Some(UNVERIFIED_DESTINATION));
    rule.manual_names.push(manual("f1", "b1-nuevo"));
    rule.draft_file_id = Some("f1".into());
    let rows = ready(calculate_preview(&entries, &rule, &mut adapter, &|| true));
    assert!(rows.iter().all(|row| row.status == Some(Status::Ready)));
}

#[test]
fn supplemental_alias_checks_detect_cycles_and_preserve_proven_occupancy() {
    let entries = files(&["a2.txt", "holder.txt", "extra.txt"]);
    let mut rule = request(text("", "", Capitalization::Keep));
    rule.manual_names = vec![manual("f0", "b1"), manual("f1", "a2"), manual("f2", "ü1")];
    rule.draft_file_id = Some("f0".into());
    let mut adapter = Adapter::default();
    adapter
        .hits
        .insert("a2.txt".into(), selected("f0", false, false));
    adapter
        .hits
        .insert("b1.txt".into(), selected("f1", true, false));
    let rows = ready(calculate_preview(&entries, &rule, &mut adapter, &|| true));
    let draft = rows.iter().find(|row| row.file_id == "f0").unwrap();
    let holder = rows.iter().find(|row| row.file_id == "f1").unwrap();
    assert_eq!(draft.reason.as_deref(), Some(UNCERTAIN_NAMES));
    assert_eq!(holder.status, Some(Status::Conflict));
    assert_eq!(
        draft.name_edit.as_ref().unwrap().save_reason.as_deref(),
        Some(UNCERTAIN_ALIAS)
    );

    rule.manual_names.retain(|name| name.file_id != "f1");
    adapter
        .hits
        .insert("b1.txt".into(), selected("f1", true, false));
    let rows = ready(calculate_preview(&entries, &rule, &mut adapter, &|| true));
    let draft = rows.iter().find(|row| row.file_id == "f0").unwrap();
    assert_eq!(draft.reason.as_deref(), Some(UNCERTAIN_NAMES));
    assert!(draft.name_edit.as_ref().unwrap().can_save);

    for (occupancy, cause, can_save, save_reason) in [
        (
            Occupancy::Unknown(AMBIGUOUS_ALIAS),
            None,
            false,
            Some(AMBIGUOUS_ALIAS),
        ),
        (
            Occupancy::Immovable { directory: false },
            Some(OriginFailure::Replaced),
            true,
            None,
        ),
        (
            Occupancy::ReadFailed,
            Some(OriginFailure::ReadFailed),
            false,
            Some(UNVERIFIED_DESTINATION),
        ),
    ] {
        adapter.hits.insert(
            "b1.txt".into(),
            Lookup {
                occupancy,
                invalidated_source: cause.map(|failure| ("f1".into(), failure)),
            },
        );
        let rows = ready(calculate_preview(&entries, &rule, &mut adapter, &|| true));
        let draft = rows.iter().find(|row| row.file_id == "f0").unwrap();
        let holder = rows.iter().find(|row| row.file_id == "f1").unwrap();
        assert_eq!(draft.reason.as_deref(), Some(UNCERTAIN_NAMES));
        let edit = draft.name_edit.as_ref().unwrap();
        assert_eq!(edit.can_save, can_save);
        assert_eq!(edit.save_reason.as_deref(), save_reason);
        if let Some(failure) = cause {
            assert_eq!(holder.reason.as_deref(), Some(failure.reason()));
            assert!(holder.name_edit.is_none());
            assert_eq!(holder.retryable, failure == OriginFailure::ReadFailed);
        }
    }
    rule.manual_names.push(manual("f1", "a1"));
    adapter
        .hits
        .insert("b1.txt".into(), selected("f1", true, false));
    let rows = ready(calculate_preview(&entries, &rule, &mut adapter, &|| true));
    let draft = rows.iter().find(|row| row.file_id == "f0").unwrap();
    assert_eq!(
        draft.name_edit.as_ref().unwrap().save_reason.as_deref(),
        Some(UNCERTAIN_ALIAS)
    );
    adapter.finish = Some(FolderAction::Reselect);
    let result = calculate_preview(&entries, &rule, &mut adapter, &|| true);
    assert!(
        matches!(result, EditPreview::FolderFailure { action: FolderAction::Reselect, rows, .. }
        if rows.iter().all(|row| row.name_edit.is_none()))
    );
}

#[test]
fn regex_interpolation_matches_the_crate_and_checks_the_final_base() {
    for (pattern, replacement, stem) in [
        (
            "(?P<árbol>[0-9]+)(z)?",
            "${árbol}-$0-$2-${missing}-$$-${sinCierre",
            "foto12-34",
        ),
        ("([0-9])", "$1a-${1}a-$", "foto12"),
        ("^|$", "x", "foto"),
        ("(P)", "$1", "foto"),
    ] {
        let config = regex(pattern, replacement, Extension::Keep);
        let actual = config
            .build()
            .unwrap()
            .calculate(&format!("{stem}.txt"), 0, None, Ok(255))
            .unwrap();
        let compiled = ::regex::Regex::new(pattern).unwrap();
        let expected = compiled.replace_all(stem, replacement);
        if !compiled.is_match(stem) {
            assert_eq!(actual, Candidate::NoMatch);
        } else {
            assert_eq!(actual, Candidate::Proposed(format!("{expected}.txt")));
        }
    }
    for extension in [
        Extension::Keep,
        Extension::Remove,
        Extension::Change {
            value: ".mkv".into(),
        },
    ] {
        assert_eq!(
            regex("^foto$", "", extension)
                .build()
                .unwrap()
                .calculate("foto.txt", 0, None, Ok(255)),
            Err("El nombre antes de la extensión está vacío.")
        );
    }
    let case = EditRule::Regex {
        pattern: "A".into(),
        replacement: "_".into(),
        ignore_case: true,
        extension: Extension::Keep,
    };
    assert_eq!(
        case.build().unwrap().calculate("a-A.txt", 0, None, Ok(255)),
        Ok(Candidate::Proposed("_-_.txt".into()))
    );
}

#[test]
fn text_preserves_unicode_context_and_applies_affixes_after_literal_replacement() {
    for (rule, input, output) in [
        (text("aa", "x", Capitalization::Keep), "aaa.txt", "xa.txt"),
        (
            text("", "", Capitalization::Upper),
            "straße.txt",
            "STRASSE.txt",
        ),
        (text("A", "Ο", Capitalization::Lower), "AΣ.txt", "ος.txt"),
        (
            EditRule::Text {
                search: "borrador".into(),
                replacement: "revisado".into(),
                prefix: "Borrador_".into(),
                suffix: String::new(),
                capitalization: Capitalization::Upper,
                extension: Extension::Keep,
            },
            "borrador-1.pdf",
            "BORRADOR_REVISADO-1.pdf",
        ),
        (
            EditRule::Text {
                search: "ausente".into(),
                replacement: "x".into(),
                prefix: "nuevo-".into(),
                suffix: String::new(),
                capitalization: Capitalization::Upper,
                extension: Extension::Remove,
            },
            "original.txt",
            "NUEVO-ORIGINAL",
        ),
    ] {
        assert_eq!(
            rule.build().unwrap().calculate(input, 0, None, Ok(255)),
            Ok(Candidate::Proposed(output.into()))
        );
    }
    assert_eq!(
        text("foto", "", Capitalization::Keep)
            .build()
            .unwrap()
            .calculate("foto.txt", 0, None, Ok(255)),
        Err("El nombre antes de la extensión está vacío.")
    );
    let invalid = regex(
        "a",
        "b",
        Extension::Change {
            value: "m kv".into(),
        },
    );
    assert!(matches!(invalid.build(), Err(errors) if errors[0].field == RuleField::Extension));
}

#[test]
fn templates_check_counter_width_date_and_episode_compatibility() {
    let config = numbering("0", "{n:03}-{fecha}{ext}");
    let rule = config.build().unwrap();
    assert_eq!(
        rule.calculate("foto.txt", 0, Some("2026-01-01"), Ok(255)),
        Ok(Candidate::Proposed("000-2026-01-01.txt".into()))
    );
    assert!(rule.calculate("foto.txt", 0, None, Ok(255)).is_err());
    assert!(numbering("0", "{nombre}").build().is_err());
    let maximum = numbering("999999999", "{n:01}");
    assert_eq!(
        maximum
            .build()
            .unwrap()
            .calculate("foto.txt", 0, None, Ok(255)),
        Ok(Candidate::Proposed("999999999.txt".into()))
    );
    assert!(
        maximum
            .build()
            .unwrap()
            .calculate("foto.txt", 1, None, Ok(255))
            .is_err()
    );
    let invalid = EditRule::Numbering {
        start: "0".into(),
        step: "0".into(),
        width: "1".into(),
        template: "{n}".into(),
        extension: Extension::Keep,
    };
    assert!(matches!(invalid.build(), Err(errors) if errors[0].field == RuleField::Step));
    let episode = EditRule::Episode {
        rule: RuleConfig {
            template: "{nombre}{ext}".into(),
            ..RuleConfig::default()
        },
        extension: Extension::Change {
            value: "mkv".into(),
        },
    };
    assert_eq!(
        episode
            .build()
            .unwrap()
            .calculate("video.1.mp4", 0, None, Ok(255)),
        Ok(Candidate::Proposed("video.1.mkv".into()))
    );
    assert_eq!(
        episode
            .build()
            .unwrap()
            .calculate("video.mp4", 0, None, Ok(255)),
        Ok(Candidate::NoMatch)
    );

    let files = files(&["1.txt", "2.txt", "3.txt"]);
    let mut adapter = Adapter::default();
    adapter
        .origins
        .insert("f1".into(), Ok(Some("2026-01-01".into())));
    adapter
        .origins
        .insert("f2".into(), Ok(Some("2026-01-01".into())));
    adapter.hits.insert(
        "2-2026-01-01.txt".into(),
        Occupancy::Immovable { directory: false }.into(),
    );
    let rows = ready(calculate_preview(
        &files,
        &request(numbering("1", "{n}-{fecha}")),
        &mut adapter,
        &|| true,
    ));
    assert_eq!(rows[0].status, Some(Status::Error));
    assert_eq!(rows[1].status, Some(Status::Conflict));
    assert_eq!(rows[1].proposed_name.as_deref(), Some("2-2026-01-01.txt"));
    assert_eq!(rows[2].proposed_name.as_deref(), Some("3-2026-01-01.txt"));
}

#[test]
fn chains_cycles_and_exclusions_use_the_displayed_order() {
    let files = files(&["1.txt", "2.txt"]);
    let mut request = request(numbering("2", "{n}"));
    let mut folder = Adapter::default();
    folder
        .hits
        .insert("2.txt".into(), selected("f1", false, false));
    let rows = ready(calculate_preview(&files, &request, &mut folder, &|| true));
    assert!(rows.iter().all(|row| row.status == Some(Status::Ready)));

    request.excluded_ids.push("f1".into());
    folder
        .hits
        .insert("2.txt".into(), selected("f1", false, false));
    let rows = ready(calculate_preview(&files, &request, &mut folder, &|| true));
    assert_eq!(rows[0].status, Some(Status::Conflict));
    assert!(rows[0].reason.as_ref().unwrap().contains("excluido"));
    assert_eq!(rows[1].status, Some(Status::Excluded));

    request.excluded_ids.clear();
    folder
        .hits
        .insert("2.txt".into(), selected("f1", false, false));
    folder.hits.insert(
        "3.txt".into(),
        Occupancy::Immovable { directory: true }.into(),
    );
    let rows = ready(calculate_preview(&files, &request, &mut folder, &|| true));
    assert!(rows.iter().all(|row| row.status == Some(Status::Conflict)));

    request.sort.direction = Direction::Desc;
    request.config = numbering("1", "{n}");
    folder
        .hits
        .insert("1.txt".into(), selected("f0", false, false));
    folder
        .hits
        .insert("2.txt".into(), selected("f1", false, false));
    let rows = ready(calculate_preview(&files, &request, &mut folder, &|| true));
    assert_eq!(rows[0].file_id, "f1");
    assert_eq!(rows[0].proposed_name.as_deref(), Some("1.txt"));
    assert_eq!(rows[1].proposed_name.as_deref(), Some("2.txt"));
    assert!(rows.iter().all(|row| row.status == Some(Status::Ready)));
}

#[test]
fn known_duplicates_preempt_indeterminate_unicode_and_skip_probing() {
    let entries = files(&[
        "á.1.txt",
        "á.2.txt",
        "é.3.txt",
        "z.4.txt",
        "a10.5.txt",
        "B10.6.txt",
    ]);
    let scene = request(regex(r"\.[0-9]+$", "", Extension::Keep));
    let mut adapter = Adapter::default();
    let rows = ready(calculate_preview(&entries, &scene, &mut adapter, &|| true));
    let by_id = rows
        .iter()
        .map(|row| (&*row.file_id, row.status))
        .collect::<HashMap<_, _>>();
    for (id, expected) in [
        ("f0", Status::Conflict),
        ("f1", Status::Conflict),
        ("f2", Status::Error),
        ("f3", Status::Error),
        ("f4", Status::Ready),
        ("f5", Status::Ready),
    ] {
        assert_eq!(by_id[id], Some(expected), "{id}");
    }
    adapter.consulted.sort();
    assert_eq!(adapter.consulted, ["B10.txt", "a10.txt"]);

    let entries = files(&["foto.1.txt", "Foto.2.txt"]);
    let mut adapter = Adapter::default();
    let rows = ready(calculate_preview(&entries, &scene, &mut adapter, &|| true));
    assert!(rows.iter().all(|row| row.status == Some(Status::Conflict)));
    assert!(adapter.consulted.is_empty());
}

#[test]
fn case_and_aliases_preserve_known_conflicts_and_reject_unverified_release() {
    let entries = files(&["1.txt", "2.txt"]);
    let scene = request(numbering("2", "{n}"));
    let mut adapter = Adapter::default();
    adapter
        .hits
        .insert("2.txt".into(), selected("f1", true, false));
    adapter
        .hits
        .insert("3.txt".into(), selected("f0", false, false));
    let rows = ready(calculate_preview(&entries, &scene, &mut adapter, &|| true));
    assert_eq!(rows[0].status, Some(Status::Error));
    assert_eq!(rows[1].status, Some(Status::Conflict));

    for alias in [true, false] {
        adapter
            .hits
            .insert("2.txt".into(), selected("f1", alias, false));
        adapter.hits.insert(
            "3.txt".into(),
            Occupancy::Immovable { directory: false }.into(),
        );
        let rows = ready(calculate_preview(&entries, &scene, &mut adapter, &|| true));
        assert!(rows.iter().all(|row| row.status == Some(Status::Conflict)));
    }

    let entries = files(&["foto.txt"]);
    let scene = request(text("f", "F", Capitalization::Keep));
    for (name, case_only, alias, status) in [
        ("Foto.txt", true, false, Status::Ready),
        ("Foto.txt", false, false, Status::Error),
        ("Foto.txt", false, true, Status::Error),
    ] {
        adapter
            .hits
            .insert(name.into(), selected("f0", alias, case_only));
        let rows = ready(calculate_preview(&entries, &scene, &mut adapter, &|| true));
        assert_eq!(rows[0].status, Some(status));
    }

    let entries = files(&["á.txt"]);
    let scene = request(text("", "", Capitalization::Upper));
    adapter
        .hits
        .insert("Á.txt".into(), selected("f0", false, false));
    let rows = ready(calculate_preview(&entries, &scene, &mut adapter, &|| true));
    assert_eq!(rows[0].status, Some(Status::Error));

    let entries = files(&["host-A.txt", "source-B.txt"]);
    let mut scene = request(regex("^.*-([AB])$", "$1", Extension::Keep));
    for direction in [Direction::Asc, Direction::Desc] {
        scene.sort.direction = direction;
        adapter
            .hits
            .insert("B.txt".into(), selected("f0", true, false));
        adapter.hits.insert(
            "A.txt".into(),
            Occupancy::Immovable { directory: false }.into(),
        );
        let rows = ready(calculate_preview(&entries, &scene, &mut adapter, &|| true));
        assert!(rows.iter().all(|row| row.status == Some(Status::Conflict)));
    }
}

#[test]
fn source_replacement_during_lookup_invalidates_its_own_plan_and_dependents() {
    let files = files(&["1.txt", "2.txt"]);
    let scene = request(numbering("2", "{n}"));
    for (occupancy, status, origin) in [
        (
            Occupancy::Immovable { directory: false },
            Status::Conflict,
            OriginFailure::Replaced,
        ),
        (
            Occupancy::Unknown(UNVERIFIED_DESTINATION),
            Status::Error,
            OriginFailure::ReadFailed,
        ),
    ] {
        let mut adapter = Adapter::default();
        adapter.hits.insert(
            "2.txt".into(),
            Lookup {
                occupancy,
                invalidated_source: Some(("f1".into(), origin)),
            },
        );
        let rows = ready(calculate_preview(&files, &scene, &mut adapter, &|| true));
        assert_eq!(rows[0].status, Some(status));
        assert_eq!(rows[1].status, Some(Status::Error));
        assert_eq!(rows[1].reason.as_deref(), Some(origin.reason()));
    }
}

#[test]
fn validates_utf16_budget_only_for_actual_changes_and_preserves_origin_precedence() {
    let files = files(&["😀a.txt"]);
    let mut adapter = Adapter {
        profile: Ok(Profile {
            prefix_units: 252,
            component_limit: 255,
        }),
        ..Adapter::default()
    };
    let same = request(regex("a", "a", Extension::Keep));
    let changed = request(regex("a", "b", Extension::Keep));
    let over = request(regex("a", "bb", Extension::Keep));
    let rows = ready(calculate_preview(&files, &changed, &mut adapter, &|| true));
    assert_eq!(rows[0].status, Some(Status::Ready));
    let rows = ready(calculate_preview(&files, &over, &mut adapter, &|| true));
    assert_eq!(rows[0].reason.as_deref(), Some(LONG_NAME));
    let mut draft = request(regex("a", "a", Extension::Keep));
    draft.draft_file_id = Some("f0".into());
    for (base, prefix_units, expected) in [
        ("😀b", 252, Status::Ready),
        ("😀b", 253, Status::Error),
        ("😀a", 253, Status::Unchanged),
    ] {
        adapter.profile = Ok(Profile {
            prefix_units,
            component_limit: 255,
        });
        draft.manual_names = vec![manual("f0", base)];
        let rows = ready(calculate_preview(&files, &draft, &mut adapter, &|| true));
        assert_eq!(rows[0].status, Some(expected));
        assert_eq!(
            rows[0].name_edit.as_ref().unwrap().can_save,
            expected != Status::Error
        );
    }
    for profile in [
        Ok(Profile {
            prefix_units: 253,
            component_limit: 255,
        }),
        Ok(Profile {
            prefix_units: 10,
            component_limit: 6,
        }),
        Err(UNVERIFIED_PROFILE),
        Err(UNSUPPORTED_PROFILE),
    ] {
        adapter.profile = profile;
        let rows = ready(calculate_preview(&files, &same, &mut adapter, &|| true));
        assert_eq!(rows[0].status, Some(Status::Unchanged));
        let rows = ready(calculate_preview(&files, &changed, &mut adapter, &|| true));
        assert_eq!(rows[0].status, Some(Status::Error));
    }
    adapter
        .origins
        .insert("f0".into(), Err(OriginFailure::Missing));
    let absent = request(regex("^x$", "", Extension::Keep));
    let rows = ready(calculate_preview(&files, &absent, &mut adapter, &|| true));
    assert_eq!(rows[0].reason.as_deref(), Some(MISSING_ORIGIN));
}

#[test]
fn invalid_rules_skip_the_folder_and_sort_retains_excluded_rows() {
    let mut files = files(&["a10.txt", "a2.txt", "a1.txt", "A2.txt"]);
    files[2].size = 10;
    files[0].kind = "MP4".into();
    let mut adapter = Adapter::default();
    let mut scene = request(regex("", "", Extension::Keep));
    scene.sort = Sort {
        column: Column::Size,
        direction: Direction::Desc,
    };
    scene.excluded_ids.push("f1".into());
    let EditPreview::Invalid { rows, errors } =
        calculate_preview(&files, &scene, &mut adapter, &|| true)
    else {
        panic!()
    };
    assert_eq!(errors[0].field, RuleField::Pattern);
    assert_eq!(
        rows.iter()
            .map(|row| row.file_id.as_str())
            .collect::<Vec<_>>(),
        ["f3", "f1", "f0", "f2"]
    );
    assert_eq!(rows[1].status, Some(Status::Excluded));
    assert_eq!(rows[0].status, None);
    assert_eq!(adapter.reads, 0);

    scene.config = numbering("0", "{n}");
    scene.sort = Sort {
        column: Column::Type,
        direction: Direction::Asc,
    };
    let rows = ready(calculate_preview(&files, &scene, &mut adapter, &|| true));
    assert_eq!(rows[0].file_id, "f0");
    for action in [FolderAction::Retry, FolderAction::Reselect] {
        adapter.action = Some(action);
        assert!(
            matches!(calculate_preview(&files, &scene, &mut adapter, &|| true),
            EditPreview::FolderFailure { action: actual, rows, .. }
            if actual == action && rows.iter().any(|row| row.status == Some(Status::Excluded)))
        );
    }
    assert!(matches!(
        calculate_preview(&files, &scene, &mut adapter, &|| false),
        EditPreview::Stale
    ));
}

#[cfg(windows)]
mod native {
    use super::*;
    use renombrador_core::infrastructure::{
        folder::{NativeFolder, NativeSelection},
        selection::read_selection,
    };
    use std::{
        collections::BTreeMap,
        fs,
        os::windows::fs::OpenOptionsExt,
        path::Path,
        time::{Duration, Instant, UNIX_EPOCH},
    };

    #[test]
    fn native_preview_probes_ntfs_leaves_the_inventory_intact_and_handles_folder_replacement() {
        let base = tempfile::tempdir().unwrap();
        let directory = base.path().join("origin");
        fs::create_dir(&directory).unwrap();
        let paths = [directory.join("1.txt"), directory.join("2.txt")];
        for (index, path) in paths.iter().enumerate() {
            fs::write(path, index.to_string()).unwrap();
        }
        let locked = fs::OpenOptions::new()
            .write(true)
            .share_mode(0)
            .open(&paths[0])
            .unwrap();
        let selected = NativeSelection::capture(read_selection(&paths).unwrap()).unwrap();
        let request = request(numbering("2", "{n}"));
        let rows = ready(calculate_preview(
            &selected.files,
            &request,
            &mut NativeFolder::new(&selected),
            &|| true,
        ));
        assert!(
            rows.iter().all(|row| row.status == Some(Status::Ready)),
            "{rows:?}"
        );
        fs::create_dir(directory.join("3.txt")).unwrap();
        let rows = ready(calculate_preview(
            &selected.files,
            &request,
            &mut NativeFolder::new(&selected),
            &|| true,
        ));
        assert!(
            rows.iter().all(|row| row.status == Some(Status::Conflict)),
            "{rows:?}"
        );
        drop(locked);
        assert_eq!(fs::read_to_string(&paths[0]).unwrap(), "0");
        assert_eq!(fs::read_to_string(&paths[1]).unwrap(), "1");
        fs::remove_dir(directory.join("3.txt")).unwrap();
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 2);

        fs::rename(&directory, base.path().join("original")).unwrap();
        fs::create_dir(&directory).unwrap();
        assert!(matches!(
            calculate_preview(
                &selected.files,
                &request,
                &mut NativeFolder::new(&selected),
                &|| true
            ),
            EditPreview::FolderFailure {
                action: FolderAction::Reselect,
                ..
            }
        ));
    }

    #[test]
    fn native_case_only_rename_checks_its_own_entry_and_keeps_hard_links_distinct() {
        let folder = tempfile::tempdir().unwrap();
        let first = folder.path().join("foto.txt");
        let second = folder.path().join("otro.txt");
        fs::write(&first, "contenido").unwrap();
        fs::hard_link(&first, &second).unwrap();
        let selected = NativeSelection::capture(read_selection(&[first, second]).unwrap()).unwrap();
        let rule = request(text("f", "F", Capitalization::Keep));
        let rows = ready(calculate_preview(
            &selected.files,
            &rule,
            &mut NativeFolder::new(&selected),
            &|| true,
        ));
        assert_eq!(rows[0].status, Some(Status::Ready), "{rows:?}");
        assert_eq!(rows[0].proposed_name.as_deref(), Some("Foto.txt"));
        assert_eq!(rows[1].status, Some(Status::Unchanged));
    }

    #[test]
    fn native_preview_uses_windows_local_modification_date() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("video.1.mp4");
        let file = fs::File::create(&path).unwrap();
        let utc = UNIX_EPOCH + Duration::from_secs(1767322800);
        file.set_times(fs::FileTimes::new().set_modified(utc))
            .unwrap();
        drop(file);
        let selected = NativeSelection::capture(read_selection(&[path]).unwrap()).unwrap();
        let rows = ready(calculate_preview(
            &selected.files,
            &request(numbering("1", "{fecha}-{n}")),
            &mut NativeFolder::new(&selected),
            &|| true,
        ));
        let proposed = rows[0].proposed_name.as_deref().unwrap();
        assert!(
            matches!(proposed, "2026-01-01-1.mp4" | "2026-01-02-1.mp4"),
            "{rows:?}"
        );
        println!("Fecha de Windows para 2026-01-02T03:00:00Z: {proposed}");
    }

    #[test]
    fn native_ntfs_counts_utf16_and_accepts_an_exact_match_on_an_existing_long_path() {
        let folder = tempfile::tempdir().unwrap();
        let prefix = folder.path().to_str().unwrap().encode_utf16().count() + 1;
        let within = format!("😀{}", "a".repeat(259 - prefix - 6));
        let beyond = format!("{within}b");
        let source = folder.path().join(format!("{beyond}.txt"));
        fs::write(&source, "contenido").unwrap();
        let selected = NativeSelection::capture(read_selection(&[source]).unwrap()).unwrap();

        for (replacement, status) in [
            (beyond.as_str(), Status::Unchanged),
            (within.as_str(), Status::Ready),
            ("", Status::Error),
        ] {
            let rule = request(regex("^.+$", replacement, Extension::Keep));
            let rows = ready(calculate_preview(
                &selected.files,
                &rule,
                &mut NativeFolder::new(&selected),
                &|| true,
            ));
            assert_eq!(rows[0].status, Some(status), "{rows:?}");
        }
        let different = format!("{within}c");
        let rule = request(regex("^.+$", &different, Extension::Keep));
        let rows = ready(calculate_preview(
            &selected.files,
            &rule,
            &mut NativeFolder::new(&selected),
            &|| true,
        ));
        assert_eq!(rows[0].reason.as_deref(), Some(LONG_NAME), "{rows:?}");
    }

    #[test]
    fn native_preview_measures_a_thousand_files_without_altering_names_or_contents() {
        fn inventory(folder: &Path) -> BTreeMap<String, Vec<u8>> {
            folder
                .read_dir()
                .unwrap()
                .map(|entry| {
                    let entry = entry.unwrap();
                    (
                        entry.file_name().into_string().unwrap(),
                        fs::read(entry.path()).unwrap(),
                    )
                })
                .collect()
        }

        let folder = tempfile::tempdir().unwrap();
        let paths = (1..=1000)
            .rev()
            .map(|index| {
                let path = folder.path().join(format!("video.{index}.mp4"));
                fs::write(&path, index.to_string()).unwrap();
                path
            })
            .collect::<Vec<_>>();
        for index in 1..=200 {
            fs::write(
                folder.path().join(format!("extra.{index}.txt")),
                "sin seleccionar",
            )
            .unwrap();
        }
        let before = inventory(folder.path());
        let selected = NativeSelection::capture(read_selection(&paths).unwrap()).unwrap();
        let rule = request(EditRule::Episode {
            rule: RuleConfig::default(),
            extension: Extension::Keep,
        });
        let start = Instant::now();
        let rows = ready(calculate_preview(
            &selected.files,
            &rule,
            &mut NativeFolder::new(&selected),
            &|| true,
        ));
        let elapsed = start.elapsed();
        assert_eq!(rows.len(), 1000);
        assert!(rows.iter().all(|row| row.status == Some(Status::Ready)));
        assert!(
            rows.iter()
                .all(|row| row.proposed_name.as_ref().unwrap().is_ascii())
        );
        assert_eq!(inventory(folder.path()), before);
        println!(
            "1000 archivos, {} entradas, 0 candidatas Unicode: {:.1} ms de cálculo",
            before.len(),
            elapsed.as_secs_f64() * 1000.0
        );

        let unicode_folder = tempfile::tempdir().unwrap();
        let paths = (0..1000)
            .map(|index| {
                let path = unicode_folder
                    .path()
                    .join(format!("á{index:04}-origen.txt"));
                fs::write(&path, index.to_string()).unwrap();
                path
            })
            .collect::<Vec<_>>();
        let before = inventory(unicode_folder.path());
        let selected = NativeSelection::capture(read_selection(&paths).unwrap()).unwrap();
        let first = &selected.files[0].id;
        let mut rule = request(text("-origen", "-nuevo", Capitalization::Keep));
        rule.manual_names = vec![manual(first, "á0000-final")];
        rule.draft_file_id = Some(first.clone());
        let start = Instant::now();
        let rows = ready(calculate_preview(
            &selected.files,
            &rule,
            &mut NativeFolder::new(&selected),
            &|| true,
        ));
        let elapsed = start.elapsed();
        assert_eq!(rows.len(), 1000);
        let draft = rows.iter().find(|row| row.file_id == *first).unwrap();
        assert_eq!(draft.proposed_name.as_deref(), Some("á0000-final.txt"));
        assert!(draft.name_edit.as_ref().unwrap().can_save);
        assert!(rows.iter().all(|row| row.status == Some(Status::Ready)));
        assert_eq!(inventory(unicode_folder.path()), before);
        println!(
            "1000 archivos Unicode y un borrador: {:.1} ms de cálculo",
            elapsed.as_secs_f64() * 1000.0
        );
    }
}
