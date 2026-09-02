pub mod commands;
pub mod db;
pub mod domain;
pub mod error;
pub mod repo;

use tauri::Manager;

use crate::commands::AppState;
use crate::db::Db;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // The database belongs to the installed app, not to the bundle:
            // ~/Library/Application Support/io.tymio.hr on macOS, the
            // equivalent per-user directory elsewhere. An installed .app is
            // read-only and code-signed, and the HR records have to outlive
            // upgrades and reinstalls.
            let data_dir = app.path().app_data_dir()?;
            let db = tauri::async_runtime::block_on(Db::open_in(&data_dir))?;
            app.manage(AppState::new(db));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::create_project,
            commands::get_project,
            commands::list_projects,
            commands::update_project,
            commands::delete_project,
            commands::portfolio_stats,
            commands::project_stats,
            commands::project_holidays,
            commands::add_project_holiday,
            commands::remove_project_holiday,
            commands::recent_activity,
            commands::create_employee,
            commands::get_employee,
            commands::list_employees,
            commands::update_employee,
            commands::delete_employee,
            commands::employee_stats,
            commands::attendance_sheet,
            commands::record_attendance,
            commands::attendance_entry,
            commands::clear_attendance,
            commands::fill_attendance_from_schedule,
            commands::employee_attendance,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
