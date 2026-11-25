use crate::state::{PreviewResponse, Selection, SelectionState};
use renombrador_core::{
    application::{PreviewRequest, Revision},
    infrastructure::{folder::NativeSelection, selection},
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
                selection::read_selection(&paths).and_then(NativeSelection::capture)
            })
    })
    .await
    .map_err(|_| "No se pudo abrir el selector de archivos.".to_owned())?;

    Ok(match chosen {
        None => PickResult::Cancelled,
        Some(Err(reason)) => PickResult::Rejected { reason },
        Some(Ok(selection)) => PickResult::Replaced {
            selection: state.replace(selection),
        },
    })
}

#[tauri::command]
pub fn notify_preview_revision(state: State<'_, SelectionState>, request: Revision) {
    state.notify(request);
}

#[tauri::command]
pub async fn generate_preview(
    state: State<'_, SelectionState>,
    request: PreviewRequest,
) -> Result<PreviewResponse, &'static str> {
    state.preview(request).await
}
