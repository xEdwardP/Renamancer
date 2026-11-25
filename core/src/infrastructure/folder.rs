#[cfg(windows)]
pub use super::windows::{NativeFolder, NativeSelection};

#[cfg(not(windows))]
pub use unsupported::{NativeFolder, NativeSelection};

#[cfg(not(windows))]
mod unsupported {
    use crate::application::{
        FileEntry,
        folder::{
            Folder, FolderAction, Inventory, Lookup, Occupancy, ReadFailure, UNSUPPORTED_PROFILE,
        },
    };

    #[derive(Debug)]
    pub struct NativeSelection {
        pub files: Vec<FileEntry>,
    }

    impl NativeSelection {
        pub fn capture(_files: Vec<FileEntry>) -> Result<Self, String> {
            Err(UNSUPPORTED_PROFILE.into())
        }
    }

    pub struct NativeFolder;

    impl NativeFolder {
        pub fn new(_selection: &NativeSelection) -> Self {
            Self
        }
    }

    impl Folder for NativeFolder {
        fn read(
            &mut self,
            _: &[&FileEntry],
            _: bool,
            _: &dyn Fn() -> bool,
        ) -> Result<Inventory, ReadFailure> {
            Err(ReadFailure::Folder(FolderAction::Reselect))
        }

        fn lookup(&mut self, _: &str, _: &FileEntry) -> Lookup {
            Occupancy::Unknown(UNSUPPORTED_PROFILE).into()
        }

        fn verify(&self) -> Result<(), FolderAction> {
            Err(FolderAction::Reselect)
        }
    }
}
