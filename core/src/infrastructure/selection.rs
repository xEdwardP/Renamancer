use crate::{application::FileEntry, domain::file_name};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

pub fn read_selection(paths: &[PathBuf]) -> Result<Vec<FileEntry>, String> {
    if paths.is_empty() {
        return Err("Selecciona al menos un archivo.".into());
    }

    let mut folder = None;
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for path in paths {
        let (resolved, metadata) = inspect(path)?;
        let parent = resolved
            .parent()
            .ok_or("No se pudo identificar la carpeta del archivo.")?;
        if folder.get_or_insert_with(|| parent.to_path_buf()) != parent {
            return Err("Selecciona archivos de una misma carpeta local.".into());
        }
        if seen.insert(resolved.clone()) {
            let name = resolved
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("No se pudo mostrar uno de los nombres seleccionados.")?
                .to_owned();
            let (_, extension) = file_name::split(&name);
            let kind = extension
                .strip_prefix('.')
                .filter(|extension| !extension.is_empty())
                .map_or_else(|| "Sin extensión".into(), str::to_uppercase);
            files.push(FileEntry {
                path: resolved,
                name,
                size: metadata.len(),
                kind,
            });
        }
    }
    files.sort_by(|left, right| {
        natord::compare_ignore_case(&left.name, &right.name)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(files)
}

fn inspect(path: &Path) -> Result<(PathBuf, fs::Metadata), String> {
    if !path.is_absolute() {
        return Err("Selecciona archivos de una misma carpeta local.".into());
    }
    #[cfg(windows)]
    if !is_local(path) {
        return Err("Selecciona archivos de un volumen local.".into());
    }

    let mut metadata = None;
    // Walk from the root to avoid inspecting anything through a junction or link.
    let ancestors: Vec<_> = path.ancestors().collect();
    for component in ancestors
        .into_iter()
        .rev()
        .filter(|part| !part.as_os_str().is_empty())
    {
        let attributes = fs::symlink_metadata(component).map_err(|_| unavailable(path))?;
        if attributes.file_type().is_symlink() {
            return Err(
                "La selección contiene un enlace simbólico o una unión de directorio.".into(),
            );
        }
        metadata = Some(attributes);
    }
    let metadata = metadata
        .filter(fs::Metadata::is_file)
        .ok_or("Selecciona únicamente archivos regulares.")?;
    let resolved = fs::canonicalize(path).map_err(|_| unavailable(path))?;
    #[cfg(windows)]
    if !is_local(&resolved) {
        return Err("Selecciona archivos de un volumen local.".into());
    }
    Ok((resolved, metadata))
}

fn unavailable(path: &Path) -> String {
    let name = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy();
    format!("No se pudieron consultar los datos de «{name}».")
}

#[cfg(windows)]
fn is_local(path: &Path) -> bool {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDriveTypeW(root: *const u16) -> u32;
    }

    is_local_with(path, |drive| {
        let root = [u16::from(drive), b':' as u16, b'\\' as u16, 0];
        // SAFETY: root is a NUL-terminated UTF-16 drive root, valid for the call.
        unsafe { GetDriveTypeW(root.as_ptr()) }
    })
}

#[cfg(windows)]
fn is_local_with(path: &Path, drive_type: impl FnOnce(u8) -> u32) -> bool {
    use std::path::{Component, Prefix};

    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return false;
    };
    let drive = match prefix.kind() {
        Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
        _ => return false,
    };
    matches!(drive_type(drive), 2 | 3 | 5 | 6)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn mapped_network_drives_are_not_local_volumes() {
        assert!(!is_local_with(
            Path::new(r"Z:\videos\video.1.mp4"),
            |drive| {
                assert_eq!(drive, b'Z');
                4 // DRIVE_REMOTE
            }
        ));
        assert!(is_local_with(Path::new(r"C:\videos\video.1.mp4"), |_| 3));
    }
}
