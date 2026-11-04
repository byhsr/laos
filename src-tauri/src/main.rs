#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod agents;
mod agent_memory;
mod anthropic;
mod app_settings;
mod brain;
mod browser;
mod capture;
mod chat;
mod classify;
mod connections;
mod db;
mod destinations;
mod embed;
mod eval;
mod fallback;
mod fox;
mod http;
mod insights;
mod integrations;
mod knowledge;
mod manager;
mod mcp;
mod media;
mod memory;
mod mic;
mod models;
mod notify;
mod notifications;
mod provider;
mod refile;
mod reminders;
mod retrieve;
mod routing;
mod scratchpad;
mod storage;
mod tasks;
mod telegram;
mod terminal;
mod tg_markdown;
mod tools;
mod tooltext;
mod transcribe;
mod tz;
mod updater;
mod vault;
mod voice;
mod workflows;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};

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
    // OS toasts for reminders, insights and finished background work.
    .plugin(tauri_plugin_notification::init())
    // Native folder / file pickers (skills import & export).
    .plugin(tauri_plugin_dialog::init())
    // The global voice hotkey: bring the window forward, then tell the frontend
    // to toggle live voice mode. Registered below from the persisted setting.
    .plugin(
      tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
          if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
            show_main(app);
            let _ = app.emit("voice-toggle", ());
          }
        })
        .build(),
    )
    .setup(|app| {
      let handle = app.handle().clone();
      // System tray: keep the app alive in the background.
      let _ = build_tray(&handle);
      // Register the saved voice hotkey (empty = disabled). A conflict is logged
      // rather than fatal; the user can change it in Settings.
      {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let accel = voice::hotkey(&handle);
        if !accel.is_empty() {
          if let Err(e) = handle.global_shortcut().register(accel.as_str()) {
            eprintln!("voice hotkey “{accel}” not registered: {e}");
          }
        }
      }
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
      // The embedded mini-browser owns its webview; hold the handle so browser
      // tools/commands can reach it.
      browser::init(app.handle().clone());
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
      storage::test_tool,
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
      // The knowledge base's re-file pass: docs that are really memories or todos.
      refile::refile_knowledge_preview,
      refile::refile_knowledge_apply,
      storage::list_skills,
      storage::save_skill,
      storage::delete_skill,
      storage::import_skills,
      storage::import_skill_files,
      storage::export_skills,
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
      integrations::refresh_google_token,
      // MCP connector (stdio)
      mcp::list_mcp_servers,
      mcp::save_mcp_server,
      mcp::delete_mcp_server,
      mcp::test_mcp_server,
      mcp::import_mcp_tools,
      mcp::call_mcp_tool,
      // Embedded mini browser (visible browser control)
      browser::browser_open,
      browser::browser_navigate,
      browser::browser_back,
      browser::browser_forward,
      browser::browser_reload,
      browser::browser_close,
      browser::browser_destroy,
      browser::browser_set_bounds,
      browser::browser_set_theme,
      browser::browser_state,
      browser::browser_eval_result,
      // Microphone permission (the webview remembers a denial per origin)
      mic::reset_microphone_permission,
      // Live voice mode: utterance transcription, reply speech, hotkey settings
      voice::voice_transcribe,
      voice::voice_speak,
      voice::set_voice_hotkey,
      // Agent + workflow execution
      agents::execute_agent,
      workflows::execute_workflow,
      workflows::list_workflow_runs,
      // Tasks + runs
      tasks::list_all_tasks,
      tasks::get_task,
      tasks::run_task,
      tasks::cancel_task,
      tasks::update_task_result,
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
      agent_memory::am_list_scopes,
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
      // The agent's own per-chat scratchpad (read-only peek for the UI).
      scratchpad::get_scratchpad,
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
      telegram::telegram_send_test,
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
      app_settings::set_desktop_notifications,
      app_settings::set_voice_tts_mode,
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
      // Notification queue (the durable inbox behind reminders/insights/finished work)
      notifications::list_notifications,
      notifications::notification_unread_count,
      notifications::mark_notification_read,
      notifications::dismiss_notification,
      notifications::mark_all_notifications_read,
      notifications::clear_notifications,
      // Learning layer (insights)
      insights::insight_list,
      insights::insight_accept,
      insights::insight_dismiss,
      insights::reflect_now,
      // Destinations (push items to an MCP server: e.g. a Notion account)
      destinations::list_destinations,
      destinations::save_destination,
      destinations::delete_destination,
      destinations::destination_tools,
      destinations::test_destination,
      destinations::push_item,
      // Brain map (the personal context engine: nodes, edges, observations)
      brain::brain_graph,
      brain::brain_node,
      brain::brain_stats,
      brain::brain_forget_node,
      brain::brain_list_observations,
      brain::brain_correct_observation,
      brain::brain_dismiss_observation,
      brain::brain_accept_observation,
      brain::brain_reindex,
      brain::brain_reflect_now,
      // Hybrid retrieval (shared by chat, agents, workflows and the UI)
      retrieve::retrieve_context,
      // Evaluation harness (quality + reliability metrics)
      eval::brain_eval,
      // Model roles (classifier / embedding)
      storage::list_model_roles,
      storage::set_model_role,
      // Provider adapters (built-in + custom, Settings → Providers)
      storage::list_providers,
      storage::save_provider,
      storage::delete_provider,
      // Resource / connection / adapter layer
      connections::list_adapters,
      connections::list_connections,
      connections::list_resources,
      connections::save_resource,
      connections::delete_resource,
    ])
    .build(tauri::generate_context!())
    .expect("error while running Local Agent OS")
    .run(|_app, event| {
      // MCP servers now run as long-lived child processes, so stop them explicitly
      // on exit instead of leaving them orphaned.
      if let tauri::RunEvent::Exit = event {
        mcp::shutdown_all();
        browser::shutdown();
      }
    });
}
