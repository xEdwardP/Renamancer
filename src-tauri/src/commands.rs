use crate::state::{Selection, SelectionState};
use renombrador_core::{
    application::{self, Preview},
    domain::rules::RuleConfig,
    infrastructure::selection,
};
use serde::Serialize;
use tauri::{State, WebviewWindow, async_runtime::spawn_blocking};
use tauri_plugin_dialog::DialogExt;

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum PickResult {
    Cancelled,
    Rejected { reason: String },
    Replaced { selection: Selection },
}

#[derive(Serialize)]
pub struct PreviewResponse {
    revision: u32,
    preview: Preview,
}

#[tauri::command]
pub async fn pick_files(
    window: WebviewWindow,
    state: State<'_, SelectionState>,
) -> Result<PickResult, String> {
    let chosen = spawn_blocking(move || {
        window
            .dialog()
            .file()
            .set_parent(&window)
            .set_title("Seleccionar archivos")
            .blocking_pick_files()
            .map(|files| {
                let paths = files
                    .into_iter()
                    .map(|file| file.into_path())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| "Selecciona únicamente archivos locales.".to_owned())?;
                selection::read_selection(&paths)
            })
    })
    .await
    .map_err(|_| "No se pudo abrir el selector de archivos.".to_owned())?;

    Ok(match chosen {
        None => PickResult::Cancelled,
        Some(Err(reason)) => PickResult::Rejected { reason },
        Some(Ok(files)) => PickResult::Replaced {
            selection: state.replace(files),
        },
    })
}

#[tauri::command]
pub async fn generate_preview(
    state: State<'_, SelectionState>,
    revision: u32,
    config: RuleConfig,
) -> Result<PreviewResponse, String> {
    let selection = state.current();
    let preview = if selection.revision == revision {
        spawn_blocking(move || application::generate_preview(&selection.files, &config))
            .await
            .map_err(|_| "No se pudo calcular la vista previa.".to_owned())?
    } else {
        Preview::Stale
    };
    Ok(PreviewResponse { revision, preview })
}
