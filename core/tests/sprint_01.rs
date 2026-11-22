use renombrador_core::{
    application::{Preview, PreviewRow, generate_preview},
    domain::rules::{RenameRule, RuleConfig, RuleField, build_rule},
    infrastructure::selection::read_selection,
};
use std::{collections::BTreeMap, fs, path::Path};

#[test]
fn episode_handles_different_files_and_validates_names() {
    let config = RuleConfig::default();
    let rule = build_rule(&config).unwrap();
    for (original, expected) in [
        ("Serie.2024.7.mkv", Some("S01E07.mkv")),
        ("Capítulo especial.7.MKV", Some("S01E07.MKV")),
        ("informe.2.pdf", Some("S01E02.pdf")),
        ("archivo7", Some("S01E07")),
        ("informe.0007.pdf", Some("S01E07.pdf")),
        ("informe.0000.pdf", Some("S01E00.pdf")),
        ("informe.120.pdf", Some("S01E120.pdf")),
        ("informe.1234567890.pdf", None),
        ("S01E07.mkv", None),
        ("carátula.jpg", None),
    ] {
        assert_eq!(
            rule.apply(original).unwrap().as_deref(),
            expected,
            "{original}"
        );
    }

    for (template, output) in [("nuevo.mkv", "nuevo.mkv.mp4"), ("VIDEO.1", "VIDEO.1.mp4")] {
        let config = RuleConfig {
            template: template.into(),
            ..RuleConfig::default()
        };
        assert_eq!(
            build_rule(&config)
                .unwrap()
                .apply("video.1.mp4")
                .unwrap()
                .as_deref(),
            Some(output)
        );
    }
    for template in [
        "{ext}",
        "CON",
        "lpt¹",
        "ruta/archivo",
        "ruta\\archivo",
        "..",
        "final. ",
    ] {
        let config = RuleConfig {
            template: template.into(),
            ..RuleConfig::default()
        };
        assert!(
            build_rule(&config).unwrap().apply("video1").is_err(),
            "{template}"
        );
    }
}

#[test]
fn episode_uses_only_the_first_group_of_the_first_match() {
    for (pattern, output) in [
        ("([0-9])", Some("S01E02.mp4")),
        ("video|([0-9]+)", None),
        ("([a-z]+)", None),
        ("()", None),
    ] {
        let config = RuleConfig {
            pattern: pattern.into(),
            ..RuleConfig::default()
        };
        assert_eq!(
            build_rule(&config)
                .unwrap()
                .apply("video.27.mp4")
                .unwrap()
                .as_deref(),
            output
        );
    }
}

#[test]
fn invalid_rule_inputs_are_attributed_to_fields() {
    for (field, invalid) in [
        (RuleField::Season, vec!["", "-1", "1000"]),
        (RuleField::Digits, vec!["0", "10", "-1"]),
        (RuleField::Pattern, vec!["[", "[0-9]", "(?!x)([0-9])"]),
        (
            RuleField::Template,
            vec!["{nombre", "nombre}", "{fecha}", "{ext}{ext}", "{ext}otro"],
        ),
    ] {
        for value in invalid {
            let mut config = RuleConfig::default();
            match field {
                RuleField::Season => config.season = value.into(),
                RuleField::Digits => config.digits = value.into(),
                RuleField::Pattern => config.pattern = value.into(),
                RuleField::Template => config.template = value.into(),
            }
            let Err(errors) = build_rule(&config) else {
                panic!("Se aceptó {value}")
            };
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].field, field);
        }
    }
}

#[test]
fn test_batches_and_existing_destinations_remain_byte_for_byte_unchanged() {
    for count in [1, 12, 120] {
        let folder = tempfile::tempdir().unwrap();
        let paths = (1..=count)
            .rev()
            .map(|index| {
                let path = folder.path().join(format!("video.{index}.mp4"));
                fs::write(&path, format!("contenido original {index}")).unwrap();
                path
            })
            .collect::<Vec<_>>();
        fs::write(folder.path().join("S01E01.mp4"), "destino existente").unwrap();
        let before = inventory(folder.path());
        let files = read_selection(&paths).unwrap();
        let Preview::Ready { rows } = generate_preview(&files, &RuleConfig::default()) else {
            panic!("Vista previa inválida")
        };
        let expected = (1..=count)
            .map(|index| PreviewRow::Proposed {
                name: format!("S01E{index:02}.mp4"),
            })
            .collect::<Vec<_>>();
        assert_eq!(rows, expected);
        let config = RuleConfig {
            template: "duplicado".into(),
            ..RuleConfig::default()
        };
        let Preview::Ready { rows } = generate_preview(&files, &config) else {
            panic!("Vista previa inválida")
        };
        assert_eq!(
            rows,
            vec![
                PreviewRow::Proposed {
                    name: "duplicado.mp4".into()
                };
                count
            ]
        );
        assert_eq!(inventory(folder.path()), before);
    }
}

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

#[test]
fn selection_checks_the_whole_lot_and_uses_paths_for_identity() {
    let folder = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let first = folder.path().join("video.2.mp4");
    let second = folder.path().join("video.10.mp4");
    let no_extension = folder.path().join("Archivo");
    let other = elsewhere.path().join("other.mp4");
    for file in [&first, &no_extension, &other] {
        fs::write(file, "datos").unwrap();
    }
    fs::hard_link(&first, &second).unwrap();
    let list =
        read_selection(&[second.clone(), first.clone(), first.clone(), no_extension]).unwrap();
    let names = list
        .iter()
        .map(|entry| (&*entry.name, &*entry.kind, entry.size))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            ("Archivo", "Sin extensión", 5),
            ("video.2.mp4", "MP4", 5),
            ("video.10.mp4", "MP4", 5)
        ]
    );
    assert!(read_selection(&[first.clone(), other]).is_err());
    assert!(read_selection(&[first, folder.path().join("ausente.mp4")]).is_err());
    assert!(read_selection(&[folder.path().to_path_buf()]).is_err());
}

#[cfg(windows)]
#[test]
fn metadata_only_selection_accepts_content_locked_files_and_rejects_unc() {
    use std::os::windows::fs::OpenOptionsExt;

    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("locked.1.mp4");
    fs::write(&path, "datos").unwrap();
    let locked = fs::OpenOptions::new()
        .write(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    assert!(fs::read(&path).is_err());
    assert_eq!(read_selection(&[path]).unwrap().len(), 1);
    drop(locked);
    let error = read_selection(&[r"\\example\share\video.1.mp4".into()]).unwrap_err();
    assert!(error.contains("volumen local"));
}
