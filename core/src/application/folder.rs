use super::FileEntry;
use crate::domain::rules::LONG_NAME;
use serde::Serialize;
use std::collections::HashMap;

pub const MISSING_ORIGIN: &str = "El archivo original ya no está en la carpeta.";
pub const REPLACED_ORIGIN: &str = "El archivo original fue sustituido.";
pub const UNVERIFIED_ORIGIN: &str = "No se pudo verificar el archivo original.";
pub const UNVERIFIED_DESTINATION: &str = "No se pudo verificar el destino.";
pub const UNVERIFIED_PROFILE: &str = "No se pudo verificar el perfil de la carpeta.";
pub const UNSUPPORTED_PROFILE: &str =
    "El destino requiere una carpeta local NTFS sin distinción de mayúsculas.";
pub const AMBIGUOUS_ALIAS: &str =
    "No se pudo determinar si el destino corresponde a un alias. Corrige el nombre.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginFailure {
    Missing,
    Replaced,
    ReadFailed,
}

impl OriginFailure {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Missing => MISSING_ORIGIN,
            Self::Replaced => REPLACED_ORIGIN,
            Self::ReadFailed => UNVERIFIED_ORIGIN,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FolderAction {
    Retry,
    Reselect,
}

#[derive(Debug)]
pub enum ReadFailure {
    Folder(FolderAction),
    Stale,
}

#[derive(Clone, Copy, Debug)]
pub struct Profile {
    pub prefix_units: usize,
    pub component_limit: usize,
}

impl Profile {
    pub fn budget(self) -> Result<usize, &'static str> {
        let path_limit = 259usize.checked_sub(self.prefix_units).ok_or(LONG_NAME)?;
        Ok(path_limit.min(self.component_limit))
    }
}

pub struct Inventory {
    pub profile: Result<Profile, &'static str>,
    pub origins: HashMap<String, Result<Option<String>, OriginFailure>>,
}

pub enum Occupancy {
    Free,
    Immovable {
        directory: bool,
    },
    Selected {
        id: String,
        alias: bool,
        case_only: bool,
    },
    Unknown(&'static str),
    ReadFailed,
}

pub struct Lookup {
    pub occupancy: Occupancy,
    pub invalidated_source: Option<(String, OriginFailure)>,
}

impl From<Occupancy> for Lookup {
    fn from(occupancy: Occupancy) -> Self {
        Self {
            occupancy,
            invalidated_source: None,
        }
    }
}

pub trait Folder {
    fn read(
        &mut self,
        included: &[&FileEntry],
        dates: bool,
        current: &dyn Fn() -> bool,
    ) -> Result<Inventory, ReadFailure>;
    fn lookup(&mut self, destination: &str, source: &FileEntry) -> Lookup;
    fn verify(&self) -> Result<(), FolderAction>;
}
