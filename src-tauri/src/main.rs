#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod agents;
mod chat;
mod db;
mod http;
mod integrations;
mod manager;
mod mcp;
mod memory;
mod models;
mod storage;
mod tasks;
mod telegram;
mod tg_markdown;
mod tools;
mod updater;
mod workflows;

fn main() {
  tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_updater::Builder::new().build())
    .setup(|app| {
      let handle = app.handle().clone();
      // A tunnel from the previous session never survives a restart — clear its
      // dead webhook so long-polling takes over instead of silently stalling.
      tauri::async_runtime::spawn(telegram::telegram_reconcile_on_boot(handle.clone()));
      // Start the Telegram long-poll adapter (no-op until a token is configured).
      tauri::async_runtime::spawn(telegram::telegram_loop(handle.clone()));
      // Start the local webhook receiver (used when a tunnel is active).
      tauri::async_runtime::spawn(telegram::telegram_webhook_server(handle, 14789));
      Ok(())
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
      // Telegram
      telegram::list_telegram_logs,
      telegram::telegram_start_tunnel,
      telegram::telegram_register_webhook,
      telegram::telegram_register_custom_url,
      telegram::telegram_stop_tunnel,
      telegram::telegram_tunnel_status,
      telegram::telegram_webhook_health,
    ])
    .run(tauri::generate_context!())
    .expect("error while running Local Agent OS");
}
