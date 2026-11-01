pub mod accounts;
pub mod agents;
pub mod api;
#[cfg(windows)]
pub mod app_id;
pub mod appdata;
pub mod commands;
pub mod endpoint_test;
pub mod error;
pub mod fallback;
pub mod git;
pub mod github;
pub mod grok_usage;
pub mod harness;
pub mod install;
#[cfg(all(test, windows))]
mod no_console_e2e;
pub mod pathenv;
pub mod plan_usage;
pub mod process;
pub mod proctree;
pub mod projects;
pub mod pty;
pub mod purge;
pub mod releases;
pub mod runner;
pub mod settings;
pub mod shells;
pub mod speech;
pub mod store;
pub mod threads;
pub mod tray;
pub mod uninstall;
pub mod updater;
pub mod vault;
pub mod window;

use tauri::Manager;

/// The main window's label.
pub fn window_label() -> &'static str {
    "main"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// `gond-code path|purge-data …`: the installer's helpers, answered without starting the app. `None` = start the
/// app as usual.
pub fn subcommand() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        // `gond-code path add|remove <dir>`: the installer's PATH edit.
        Some("path") => Some(pathenv::run(&args[2..])),
        // `gond-code purge-data`: the uninstaller's "Delete the application data".
        Some("purge-data") => Some(purge::run(&args[2..])),
        _ => None,
    }
}

/// The bundle identifier from tauri.conf.json (the AppUserModelID).
pub fn tauri_identifier() -> String {
    let conf: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).unwrap_or_default();
    conf["identifier"]
        .as_str()
        .unwrap_or("app.gond.code")
        .to_string()
}

pub fn run() {
    // Same AppUserModelID as the installer's shortcut, before any window exists,
    // so the taskbar and notifications attach to this window even when uninstalled.
    #[cfg(windows)]
    app_id::set_app_id(&tauri_identifier());
    tauri::Builder::default()
        // Before setup: the page may ask for it while setup is still running.
        .manage(std::sync::Arc::new(window::InfoCell::default()))
        // First, so a second launch hands over before anything else starts.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            window::on_second_instance(app)
        }))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                // Visibility is ours: the window starts hidden and setup shows it.
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        - tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // X hides to the tray (Settings → General, on by default), so running agents keep
        // working; without a tray, or with it off, X quits.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let to_tray = window.label() == window_label()
                    && app.try_state::<tray::Tray>().is_some()
                    && app
                        .try_state::<commands::api::Api>()
                        .and_then(|a| settings::get(&a.services.store).ok())
                        .is_none_or(|s| s.close_to_tray);
                if to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            // Detection and every spawn start from the registry's PATH, not the one handed down
            // by whatever launched Gond (or the Gond it restarted from for an update).
            if let Some(path) = process::fresh_path() {
                process::set_path(path);
            }
            let path = store::default_db_path().ok_or("no app data directory")?;
            let store = store::Store::open(&path)?;
            // One vault shared by account.* (writes) and the runner (resolves at spawn).
            let vault = std::sync::Arc::new(vault::SystemVault::default());
            let runner = runner::AgentRunner::new(store.clone()).with_secrets(vault.clone());
            app.state::<std::sync::Arc<window::InfoCell>>()
                .set(window::setup(app.handle()));
            // A missing tray (some Linux desktops) isn't fatal.
            if let Ok(tray) = tray::setup(app.handle()) {
                app.manage(tray);
            }
            let sink = commands::pty::EventSink(app.handle().clone());
            let ptys = pty::PtyManager::new(std::sync::Arc::new(sink));
            let api = commands::api::Api {
                registry: api::registry(),
                services: api::Services::new(
                    store.clone(),
                    vault,
                    accounts::default_root().ok_or("no local app data directory")?,
                )?
                .with_runtime(runner.clone(), ptys.clone()),
            };
            // Looks for a newer Gond Code on start and every few hours.
            updater::start(app.handle(), std::sync::Arc::new(api.services.clone()));
            // Accounts made before identities: who they are, then duplicates merged.
            let tidy = api.services.clone();
            std::thread::spawn(move || {
                let _ = api::accounts::tidy(&tidy);
            });
            commands::api::emit_patches(app.handle(), &api);
            app.manage(api);
            app.manage(runner);
            app.manage(store);
            app.manage(ptys);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::api::command_run,
            commands::api::command_list,
            commands::api::state_get,
            commands::window::window_info,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Gond Code")
        .run(|app, event| {
            // The agents' and shells' terminals die with the app rather than linger.
            if let tauri::RunEvent::Exit = event {
                app.state::<pty::PtyManager>().close_all();
            }
        });
}
