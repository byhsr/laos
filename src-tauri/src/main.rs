#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod agents;
mod agent_memory;
mod anthropic;
mod app_settings;
mod capture;
mod chat;
mod db;
mod fallback;
mod fox;
mod http;
mod insights;
mod integrations;
mod manager;
mod mcp;
mod memory;
mod models;
mod provider;
mod reminders;
mod storage;
mod tasks;
mod telegram;
mod terminal;
mod tg_markdown;
mod tools;
mod tooltext;
mod updater;
mod workflows;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

fn show_main(app: &tauri::AppHandle) {
  if let Some(w) = app.get_webview_window("main") {
    let _ = w.show();
    let _ = w.unminimize();
    let _ = w.set_focus();
  }
}

// System tray so the app keeps running (Telegram keeps polling) when the window
// is closed to the tray: left-click or Show brings it back, Quit exits for real.
fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
  let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
  let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
  let menu = Menu::with_items(app, &[&show, &quit])?;
  let mut builder = TrayIconBuilder::with_id("lup-tray")
    .menu(&menu)
    .show_menu_on_left_click(false)
    .on_menu_event(|app, event| match event.id.as_ref() {
      "show" => show_main(app),
      "quit" => app.exit(0),
      _ => {}
    })
    .on_tray_icon_event(|tray, event| {
      if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
        show_main(tray.app_handle());
      }
    });
  if let Some(icon) = app.default_window_icon() { builder = builder.icon(icon.clone()); }
  builder.build(app)?;
  Ok(())
}

fn main() {
  tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_updater::Builder::new().build())
    .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
    .setup(|app| {
      let handle = app.handle().clone();
      // System tray: keep the app alive in the background.
      let _ = build_tray(&handle);
      // Launched by the OS at login — come up hidden in the tray.
      if std::env::args().any(|a| a == "--hidden") {
        if let Some(w) = app.get_webview_window("main") { let _ = w.hide(); }
      }
      // A tunnel from the previous session never survives a restart — clear its
      // dead webhook so long-polling takes over instead of silently stalling.
      tauri::async_runtime::spawn(telegram::telegram_reconcile_on_boot(handle.clone()));
      // Start the Telegram long-poll adapter (no-op until a token is configured).
      tauri::async_runtime::spawn(telegram::telegram_loop(handle.clone()));
      // Relay terminal-session output to bots bound to them.
      tauri::async_runtime::spawn(telegram::terminal_relay_loop(handle.clone()));
      // Fire due reminders (and recurring habits) to the bot that captured them.
      tauri::async_runtime::spawn(reminders::reminder_loop(handle.clone()));
      // The learning layer: reflect on what's been filed every half hour.
      tauri::async_runtime::spawn(insights::insight_loop(handle.clone()));
      // Start the local webhook receiver (used when a tunnel is active).
      tauri::async_runtime::spawn(telegram::telegram_webhook_server(handle, 14789));
      Ok(())
    })
    .on_window_event(|window, event| {
      if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        // Hide to the tray instead of quitting, so Telegram keeps polling.
        if app_settings::run_in_background(window.app_handle()) {
          api.prevent_close();
          let _ = window.hide();
        }
      }
    })
    .invoke_handler(tauri::generate_handler![
      // Storage: bootstrap + CRUD
      storage::initialize_storage,
      storage::list_model_configs,
      storage::save_model_config,
      storage::delete_model_config,
      storage::list_tools,
      storage::save_tool,
      storage::delete_tool,
      storage::list_agents,
      storage::save_agent,
      storage::delete_agent,
      storage::list_workflows,
      storage::save_workflow,
      storage::delete_workflow,
      storage::list_knowledge_docs,
      storage::get_knowledge_doc,
      storage::save_knowledge_doc,
      storage::delete_knowledge_doc,
      storage::list_skills,
      storage::save_skill,
      storage::delete_skill,
      // Scripts (reusable runnable commands / custom apps)
      storage::list_scripts,
      storage::save_script,
      storage::delete_script,
      storage::run_script_now,
      // Projects (group conversations + scope their memory)
      storage::list_projects,
      storage::save_project,
      storage::delete_project,
      // Integrations
      integrations::list_integrations,
      integrations::save_integration_config,
      integrations::test_integration,
      integrations::start_oauth,
      integrations::connect_oauth,
      integrations::complete_oauth,
      // MCP connector (stdio)
      mcp::list_mcp_servers,
      mcp::save_mcp_server,
      mcp::delete_mcp_server,
      mcp::test_mcp_server,
      mcp::import_mcp_tools,
      mcp::call_mcp_tool,
      // Agent + workflow execution
      agents::execute_agent,
      workflows::execute_workflow,
      workflows::list_workflow_runs,
      // Tasks + runs
      tasks::list_all_tasks,
      tasks::get_task,
      tasks::run_task,
      tasks::cancel_task,
      tasks::list_runs,
      // Manager + chat
      manager::manager_message,
      manager::confirm_manager_tool,
      chat::stream_chat,
      chat::cancel_chat,
      // Terminal sessions (PTY)
      terminal::terminal_start,
      terminal::terminal_write,
      terminal::terminal_read,
      terminal::terminal_attach,
      terminal::terminal_resize,
      terminal::terminal_list,
      terminal::terminal_kill,
      // Built-in memory connector (fox) — host-driven feedback
      fox::rate_turn,
      // Native agent memory (was the fox MCP server) — the Memory view reads these
      agent_memory::am_list_memories,
      agent_memory::am_search,
      agent_memory::am_get_self_model,
      agent_memory::am_stats,
      agent_memory::am_list_namespaces,
      agent_memory::am_add_memory,
      agent_memory::am_update_memory,
      agent_memory::am_forget_memory,
      agent_memory::am_get_memory,
      agent_memory::am_remove_self_model,
      agent_memory::am_set_self_model,
      agent_memory::am_compile_context,
      agent_memory::am_create_namespace,
      agent_memory::am_remove_namespace,
      // Memory + chat sessions
      memory::get_conversation,
      memory::clear_agent_memory,
      memory::list_chat_sessions,
      memory::get_chat_session,
      memory::create_chat_session,
      memory::delete_chat_session,
      memory::rename_chat_session,
      memory::close_session,
      // Updater (in-app, Rust-side — no JS plugin packages needed)
      updater::check_for_update,
      updater::install_update,
      updater::restart_app,
      updater::app_version,
      // Telegram
      telegram::list_telegram_logs,
      telegram::list_telegram_bots,
      telegram::save_telegram_bot,
      telegram::delete_telegram_bot,
      telegram::telegram_set_bot_webhook,
      telegram::telegram_clear_bot_webhook,
      telegram::telegram_ask,
      telegram::telegram_start_tunnel,
      telegram::telegram_register_webhook,
      telegram::telegram_register_custom_url,
      telegram::telegram_stop_tunnel,
      telegram::telegram_tunnel_status,
      telegram::telegram_webhook_health,
      // App settings (background mode + launch at login)
      app_settings::get_app_settings,
      app_settings::set_run_in_background,
      app_settings::set_launch_at_login,
      app_settings::set_default_shell,
      app_settings::set_cli_shortcuts,
      app_settings::set_autonomy,
      // Identity (who the assistant is / who it's talking to)
      app_settings::get_identity,
      app_settings::set_identity,
      // Capture (second brain)
      capture::item_list,
      capture::item_save,
      capture::item_delete,
      capture::item_toggle,
      // Reminders + habits (scheduler)
      reminders::item_due,
      reminders::reminder_snooze,
      // Learning layer (insights)
      insights::insight_list,
      insights::insight_accept,
      insights::insight_dismiss,
      insights::reflect_now,
    ])
    .build(tauri::generate_context!())
    .expect("error while running Local Agent OS")
    .run(|_app, event| {
      // MCP servers now run as long-lived child processes, so stop them explicitly
      // on exit instead of leaving them orphaned.
      if let tauri::RunEvent::Exit = event {
        mcp::shutdown_all();
      }
    });
}
