use super::selection::inspect_ancestors;
use crate::application::{
    FileEntry,
    folder::{
        AMBIGUOUS_ALIAS, Folder, FolderAction, Inventory, Lookup, Occupancy, OriginFailure,
        Profile, ReadFailure, UNSUPPORTED_PROFILE, UNVERIFIED_PROFILE,
    },
};
use std::{
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString, c_void},
    fs::{self, File, Metadata, OpenOptions},
    io,
    mem::size_of,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::{MetadataExt, OpenOptionsExt},
        io::{AsRawHandle, RawHandle},
    },
    path::{Path, PathBuf},
    ptr,
};

const FILE_READ_ATTRIBUTES: u32 = 0x80;
const FILE_SHARE_ALL: u32 = 0x7;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const FILE_ID_INFO: i32 = 18;
const FILE_CASE_SENSITIVE_INFO: i32 = 23;
const FILE_CS_FLAG_CASE_SENSITIVE_DIR: u32 = 1;
const ERROR_FILE_NOT_FOUND: i32 = 2;
const CSTR_EQUAL: i32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Identity {
    volume: u64,
    file: [u8; 16],
}

#[repr(C)]
#[derive(Default)]
struct FileTime {
    low: u32,
    high: u32,
}

#[repr(C)]
#[derive(Default)]
struct SystemTime {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}

#[repr(C)]
struct FindData {
    attributes: u32,
    creation: FileTime,
    access: FileTime,
    write: FileTime,
    size_high: u32,
    size_low: u32,
    reserved: [u32; 2],
    long_name: [u16; 260],
    short_name: [u16; 14],
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetFileInformationByHandleEx(
        handle: RawHandle,
        class: i32,
        output: *mut c_void,
        length: u32,
    ) -> i32;
    fn GetVolumeInformationByHandleW(
        handle: RawHandle,
        volume_name: *mut u16,
        volume_capacity: u32,
        serial: *mut u32,
        maximum_component: *mut u32,
        flags: *mut u32,
        file_system: *mut u16,
        system_capacity: u32,
    ) -> i32;
    fn FindFirstFileW(path: *const u16, output: *mut FindData) -> RawHandle;
    fn FindClose(search: RawHandle) -> i32;
    fn CompareStringOrdinal(
        left: *const u16,
        left_length: i32,
        right: *const u16,
        right_length: i32,
        ignore_case: i32,
    ) -> i32;
    fn FileTimeToSystemTime(time: *const FileTime, output: *mut SystemTime) -> i32;
    fn SystemTimeToTzSpecificLocalTimeEx(
        zone: *const c_void,
        utc: *const SystemTime,
        local: *mut SystemTime,
    ) -> i32;
}

#[derive(Debug)]
struct Selected {
    id: String,
    identity: Identity,
}

#[derive(Debug)]
pub struct NativeSelection {
    pub files: Vec<FileEntry>,
    directory: PathBuf,
    identity: Identity,
    entries: HashMap<OsString, Selected>,
}

impl NativeSelection {
    pub fn capture(mut files: Vec<FileEntry>) -> Result<Self, String> {
        let directory = files
            .first()
            .and_then(|file| file.path.parent())
            .ok_or("No se pudo identificar la carpeta seleccionada.")?
            .to_owned();
        let (_, identity) = open_directory(&directory)
            .map_err(|_| "No se pudo verificar la carpeta seleccionada.")?;
        let mut entries = HashMap::with_capacity(files.len());
        for file in &mut files {
            let checked = open_entry(&file.path)
                .and_then(|entry| {
                    if entry.regular() {
                        Ok(entry)
                    } else {
                        Err(io::Error::other("La entrada no es un archivo regular"))
                    }
                })
                .map_err(|_| format!("No se pudo verificar el archivo «{}».", file.name))?;
            file.size = checked.metadata.len();
            entries.insert(
                OsString::from(&file.name),
                Selected {
                    id: file.id.clone(),
                    identity: checked.identity,
                },
            );
        }
        if open_directory(&directory).map(|(_, found)| found).ok() != Some(identity) {
            return Err(
                "La carpeta cambió durante la selección. Selecciona los archivos de nuevo.".into(),
            );
        }
        Ok(Self {
            files,
            directory,
            identity,
            entries,
        })
    }
}

pub struct NativeFolder<'a> {
    selection: &'a NativeSelection,
    directory_handle: Option<File>,
}

impl<'a> NativeFolder<'a> {
    pub fn new(selection: &'a NativeSelection) -> Self {
        Self {
            selection,
            directory_handle: None,
        }
    }
}

impl Folder for NativeFolder<'_> {
    fn read(
        &mut self,
        included: &[&FileEntry],
        dates: bool,
        current: &dyn Fn() -> bool,
    ) -> Result<Inventory, ReadFailure> {
        let directory = &self.selection.directory;
        let (handle, identity) =
            open_directory(directory).map_err(|_| ReadFailure::Folder(FolderAction::Reselect))?;
        if identity != self.selection.identity {
            return Err(ReadFailure::Folder(FolderAction::Reselect));
        }
        let profile = query_profile(&handle, directory);
        self.directory_handle = Some(handle);
        let read_error =
            |_| ReadFailure::Folder(self.verify().err().unwrap_or(FolderAction::Retry));
        let mut names = HashSet::with_capacity(included.len());
        for entry in fs::read_dir(directory).map_err(read_error)? {
            if !current() {
                return Err(ReadFailure::Stale);
            }
            let name = entry.map_err(read_error)?.file_name();
            if self.selection.entries.contains_key(&name) {
                names.insert(name);
            }
        }
        let mut origins = HashMap::with_capacity(included.len());
        for file in included {
            if !current() {
                return Err(ReadFailure::Stale);
            }
            let name = OsStr::new(&file.name);
            let evidence = if names.contains(name) {
                match open_entry(&file.path) {
                    Ok(found)
                        if found.regular()
                            && found.identity == self.selection.entries[name].identity =>
                    {
                        Ok(if dates {
                            local_date(found.metadata.last_write_time())
                        } else {
                            None
                        })
                    }
                    Ok(_) => Err(OriginFailure::Replaced),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        Err(OriginFailure::Missing)
                    }
                    Err(_) => Err(OriginFailure::ReadFailed),
                }
            } else {
                Err(OriginFailure::Missing)
            };
            origins.insert(file.id.clone(), evidence);
        }
        Ok(Inventory { profile, origins })
    }

    fn lookup(&mut self, destination: &str, source: &FileEntry) -> Lookup {
        let found = match find(&self.selection.directory.join(destination)) {
            Ok(Some(found)) => found,
            Ok(None) => return Occupancy::Free.into(),
            Err(_) => return Occupancy::ReadFailed.into(),
        };
        let long = wide_name(&found.long_name);
        let name = OsString::from_wide(long);
        let Some(selected) = self.selection.entries.get(&name) else {
            return Occupancy::Immovable {
                directory: found.attributes & FILE_ATTRIBUTE_DIRECTORY != 0,
            }
            .into();
        };

        match open_entry(&self.selection.directory.join(&name)) {
            Ok(entry) if entry.regular() && entry.identity == selected.identity => {}
            Ok(entry) => {
                return Lookup {
                    occupancy: Occupancy::Immovable {
                        directory: entry.metadata.is_dir(),
                    },
                    invalidated_source: Some((selected.id.clone(), OriginFailure::Replaced)),
                };
            }
            Err(_) => {
                return Lookup {
                    occupancy: Occupancy::ReadFailed,
                    invalidated_source: Some((selected.id.clone(), OriginFailure::ReadFailed)),
                };
            }
        }
        let requested = destination.encode_utf16().collect::<Vec<_>>();
        let short = wide_name(&found.short_name);
        let long_relation = relation(&requested, long);
        let alias = if long_relation == NameRelation::Equivalent || short.is_empty() {
            false
        } else if long_relation == NameRelation::Distinct
            || relation(&requested, short) == NameRelation::Equivalent
        {
            true
        } else if relation(&requested, short) == NameRelation::Distinct {
            false
        } else {
            return Occupancy::Unknown(AMBIGUOUS_ALIAS).into();
        };
        let case_only = selected.id == source.id
            && !alias
            && requested != long
            && long_relation == NameRelation::Equivalent
            && confirm_case(&requested, long);
        Occupancy::Selected {
            id: selected.id.clone(),
            alias,
            case_only,
        }
        .into()
    }

    fn verify(&self) -> Result<(), FolderAction> {
        let (_, found) =
            open_directory(&self.selection.directory).map_err(|_| FolderAction::Reselect)?;
        if found != self.selection.identity {
            return Err(FolderAction::Reselect);
        }
        Ok(())
    }
}

struct Entry {
    identity: Identity,
    metadata: Metadata,
}

impl Entry {
    fn regular(&self) -> bool {
        self.metadata.is_file()
            && self.metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
    }
}

fn open_metadata(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_ALL)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

fn open_directory(path: &Path) -> io::Result<(File, Identity)> {
    inspect_ancestors(path).map_err(io::Error::other)?;
    let directory = open_metadata(path)?;
    let metadata = directory.metadata()?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("La entrada no es un directorio regular"));
    }
    let mut identity = Identity::default();
    query_info(&directory, FILE_ID_INFO, &mut identity)?;
    Ok((directory, identity))
}

fn open_entry(path: &Path) -> io::Result<Entry> {
    let file = open_metadata(path)?;
    let mut identity = Identity::default();
    query_info(&file, FILE_ID_INFO, &mut identity)?;
    Ok(Entry {
        identity,
        metadata: file.metadata()?,
    })
}

fn query_info<T>(file: &File, class: i32, output: &mut T) -> io::Result<()> {
    // SAFETY: Class and repr(C) output type are paired at each call. The live file
    // owns the handle and output is writable for the supplied size.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            class,
            ptr::from_mut(output).cast(),
            size_of::<T>() as u32,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn query_profile(directory: &File, path: &Path) -> Result<Profile, &'static str> {
    let mut file_system = [0u16; 32];
    let mut component_limit = 0;
    // SAFETY: The handle is live, output pointers match the declared sizes,
    // and omitted outputs accept null pointers.
    let ok = unsafe {
        GetVolumeInformationByHandleW(
            directory.as_raw_handle(),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            &mut component_limit,
            ptr::null_mut(),
            file_system.as_mut_ptr(),
            file_system.len() as u32,
        )
    };
    if ok == 0 || component_limit == 0 {
        return Err(UNVERIFIED_PROFILE);
    }
    if wide_name(&file_system) != [78, 84, 70, 83] {
        return Err(UNSUPPORTED_PROFILE);
    }
    let mut flags = 0u32;
    query_info(directory, FILE_CASE_SENSITIVE_INFO, &mut flags).map_err(|_| UNVERIFIED_PROFILE)?;
    if flags & FILE_CS_FLAG_CASE_SENSITIVE_DIR != 0 {
        return Err(UNSUPPORTED_PROFILE);
    }
    let sample = path.join("x");
    let units = sample.as_os_str().encode_wide().collect::<Vec<_>>();
    let verbatim = [92, 92, 63, 92];
    let ordinary = units.strip_prefix(&verbatim).unwrap_or(&units);
    Ok(Profile {
        prefix_units: ordinary.len() - 1,
        component_limit: component_limit as usize,
    })
}

fn find(path: &Path) -> io::Result<Option<FindData>> {
    let name = path
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    let mut data = std::mem::MaybeUninit::<FindData>::uninit();
    // SAFETY: The path is NUL-terminated and data has the layout and size of WIN32_FIND_DATAW.
    let search = unsafe { FindFirstFileW(name.as_ptr(), data.as_mut_ptr()) };
    if search as isize == -1 {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(ERROR_FILE_NOT_FOUND) {
            Ok(None)
        } else {
            Err(error)
        };
    }
    // SAFETY: A successful FindFirstFileW initialized data and yielded a search handle.
    unsafe {
        FindClose(search);
        Ok(Some(data.assume_init()))
    }
}

fn wide_name(buffer: &[u16]) -> &[u16] {
    buffer.split(|&unit| unit == 0).next().unwrap_or_default()
}

#[derive(PartialEq, Eq)]
enum NameRelation {
    Equivalent,
    Distinct,
    Indeterminate,
}

fn relation(left: &[u16], right: &[u16]) -> NameRelation {
    if left.len() != right.len() {
        return NameRelation::Distinct;
    }
    let mut uncertain = false;
    for (&left, &right) in left.iter().zip(right) {
        if left == right {
            continue;
        }
        if left <= 127 && right <= 127 {
            if !(left as u8).eq_ignore_ascii_case(&(right as u8)) {
                return NameRelation::Distinct;
            }
        } else {
            uncertain = true;
        }
    }
    if uncertain {
        NameRelation::Indeterminate
    } else {
        NameRelation::Equivalent
    }
}

fn confirm_case(left: &[u16], right: &[u16]) -> bool {
    // SAFETY: Both buffers are valid for their respective lengths and are limited
    // to Windows filename component sizes before comparison.
    unsafe {
        CompareStringOrdinal(
            left.as_ptr(),
            left.len() as i32,
            right.as_ptr(),
            right.len() as i32,
            1,
        ) == CSTR_EQUAL
    }
}

fn local_date(ticks: u64) -> Option<String> {
    let file_time = FileTime {
        low: ticks as u32,
        high: (ticks >> 32) as u32,
    };
    let mut utc = SystemTime::default();
    let mut local = SystemTime::default();
    // SAFETY: Structures have the ABI layout, outputs are writable and null asks
    // Windows to use the local dynamic time zone.
    let converted = unsafe {
        FileTimeToSystemTime(&file_time, &mut utc) != 0
            && SystemTimeToTzSpecificLocalTimeEx(ptr::null(), &utc, &mut local) != 0
    };
    (converted && (1..=9999).contains(&local.year))
        .then(|| format!("{:04}-{:02}-{:02}", local.year, local.month, local.day))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::selection::read_selection;

    #[test]
    fn short_alias_resolves_to_the_long_entry_when_available() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("Documento de muestra para Renamancer.txt");
        fs::write(&path, "contenido").unwrap();
        let hit = find(&path).unwrap().unwrap();
        let alias = String::from_utf16(wide_name(&hit.short_name)).unwrap();
        if alias.is_empty() {
            println!(
                "El volumen de prueba no generó un alias 8.3. Se verificó con adaptador simulado."
            );
            return;
        }
        let selection = NativeSelection::capture(read_selection(&[path]).unwrap()).unwrap();
        let source = &selection.files[0];
        let hit = NativeFolder::new(&selection).lookup(&alias, source);
        assert!(
            matches!(hit.occupancy, Occupancy::Selected { id, alias: true, case_only: false } if id == source.id)
        );
        println!("Alias 8.3 verificado en Windows.");
    }
}
