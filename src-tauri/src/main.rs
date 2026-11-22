#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::SelectionState::default())
        .invoke_handler(tauri::generate_handler![
            commands::pick_files,
            commands::generate_preview
        ])
        .run(tauri::generate_context!())
        .expect("No se pudo iniciar Renamancer");
}
