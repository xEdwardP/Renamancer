use renombrador_core::application::FileEntry;
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default, Serialize)]
pub struct Selection {
    pub revision: u32,
    pub files: Arc<Vec<FileEntry>>,
}

#[derive(Default)]
pub struct SelectionState(Mutex<Selection>);

impl SelectionState {
    pub fn current(&self) -> Selection {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn replace(&self, files: Vec<FileEntry>) -> Selection {
        let mut selection = self.0.lock().unwrap_or_else(|error| error.into_inner());
        *selection = Selection {
            revision: selection.revision.wrapping_add(1),
            files: Arc::new(files),
        };
        selection.clone()
    }
}
