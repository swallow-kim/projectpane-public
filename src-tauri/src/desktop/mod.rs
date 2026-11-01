mod archive_tools;
mod commands;
mod explorer;
mod focus;
mod geometry;
mod navigation;
mod operation_close;
mod operation_runtime;
mod operations;
mod project_close_gate;
mod projects;
mod runtime;
mod startup;
mod update_handoff;
mod update_installation;
#[cfg(test)]
mod update_package_tests;
mod update_policy;
mod updates;
mod watch;

use projectpane_core::{domain::StartupConfig, persistence::EventKind, services::Broker};
use runtime::DesktopState;
use startup::{StartupResponse, StartupState, Warning};
use std::{io, time::Duration};
use tauri::{Manager, RunEvent, WindowEvent};

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = StartupConfig::from_args(std::env::args_os().skip(1));
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus::focus_existing(app)
        }))
        .invoke_handler(tauri::generate_handler![
            updates::update_status,
            updates::check_update,
            updates::download_update,
            updates::cancel_update_download,
            updates::install_update,
            updates::open_update_page,
            archive_tools::list_archive_tools,
            archive_tools::launch_archive_tool,
            commands::startup_config,
            watch::poll_watch,
            commands::start_listing,
            commands::read_listing,
            commands::cancel_listing,
            commands::set_listing_view,
            commands::locate_entry,
            commands::activate_entry,
            explorer::export_to_explorer,
            explorer::import_from_explorer,
            explorer::clipboard_sequence,
            projects::list_projects,
            projects::list_favorites,
            projects::save_favorites,
            projects::create_project,
            projects::load_project,
            projects::rename_project,
            projects::duplicate_project,
            projects::remove_project,
            projects::save_project_preset,
            projects::save_project_session,
            projects::project_close_ready,
            operations::get_operation_state,
            operations::prepare_operation,
            operations::capture_transfer,
            operations::prepare_transfer,
            operations::execute_operation,
            operations::cancel_operation,
            operations::discard_operation,
            operations::get_operation_report,
            operations::forget_operation,
            operation_close::operation_close_choice
        ])
        .setup(move |app| {
            app.manage(updates::DesktopUpdates::initialize(app.handle()));
            let executable =
                std::env::current_exe().map_err(|_| io::Error::other("executable_unavailable"))?;
            let directory = executable
                .parent()
                .ok_or_else(|| io::Error::other("executable_unavailable"))?;
            let worker = directory.join(if cfg!(windows) {
                "projectpane-read-worker.exe"
            } else {
                "projectpane-read-worker"
            });
            #[cfg(windows)]
            app.manage(projectpane_core::services::watch::WatchHub::new(
                worker.clone(),
            ));
            app.manage(Broker::new(worker, Duration::from_secs(8)));
            let (sender, receiver) = tokio::sync::watch::channel(None);
            app.manage(StartupState(receiver));
            let mut warnings = Vec::new();
            let (desktop, saved) = DesktopState::initialize(app.handle(), &mut warnings);
            app.manage(desktop);
            app.manage(projects::DesktopProjects::initialize(app.handle()));
            let window_config = app
                .config()
                .app
                .windows
                .iter()
                .find(|window| window.label == "main")
                .ok_or_else(|| io::Error::other("main_config_unavailable"))?;
            let window = tauri::WebviewWindowBuilder::from_config(app, window_config)?
                .visible(false)
                .on_navigation(|url| navigation::allowed(url, cfg!(debug_assertions)))
                .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
                .build()?;
            app.manage(operation_runtime::DesktopOperations::initialize(
                app.handle(),
                &window,
            ));
            let desktop = app.state::<DesktopState>();
            let initial = geometry::normal_bounds(&window).ok().flatten();
            let normal = match saved.or(initial) {
                Some(saved) => match geometry::restore(&window, saved) {
                    Ok(bounds) => Some(bounds),
                    Err(()) => {
                        warnings.push(Warning::WindowRestoreFailed);
                        runtime::record(
                            desktop.logger.as_ref(),
                            EventKind::WindowRestoreFailed,
                            None,
                        );
                        if let Some(initial) = initial {
                            let _ = geometry::restore(&window, initial);
                        }
                        geometry::normal_bounds(&window).ok().flatten().or(initial)
                    }
                },
                None => {
                    warnings.push(Warning::WindowRestoreFailed);
                    runtime::record(
                        desktop.logger.as_ref(),
                        EventKind::WindowRestoreFailed,
                        None,
                    );
                    initial
                }
            };
            desktop.seed(normal);
            sender.send_replace(Some(StartupResponse::new(config, warnings)));
            window.show()?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                    if let (Some(state), Some(webview)) = (
                        window.try_state::<DesktopState>(),
                        window.get_webview_window("main"),
                    ) {
                        state.capture(&webview);
                    }
                }
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    operation_close::request_close(window.app_handle());
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())?;
    app.run(|app, event| {
        if let RunEvent::ExitRequested { api, .. } = &event
            && !(operation_close::exit_allowed(app) && projects::exit_allowed(app))
        {
            api.prevent_exit();
            operation_close::request_close(app);
            return;
        }
        #[cfg(windows)]
        if matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit)
            && let Some(watch) = app.try_state::<projectpane_core::services::watch::WatchHub>()
        {
            watch.close();
        }
        if matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit)
            && let Some(state) = app.try_state::<DesktopState>()
        {
            state.stop(app.get_webview_window("main").as_ref());
        }
    });
    Ok(())
}
