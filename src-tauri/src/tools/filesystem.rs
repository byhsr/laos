// Filesystem + command tools: agent-home file access, host-wide search/read,
// and shell execution. Host tools are gated behind the `host_fs` permission.

use async_trait::async_trait;
use std::fs;

use super::{clip, str_arg, AgentTool};

pub(crate) struct ReadFileTool { pub(crate) home: std::path::PathBuf }
pub(crate) struct WriteFileTool { pub(crate) home: std::path::PathBuf }

impl ReadFileTool {
  fn resolve(&self, name: &str) -> Result<std::path::PathBuf, String> {
    let p = self.home.join("files").join(name);
    if !p.starts_with(&self.home) { return Err("Path escapes agent home.".into()); }
    Ok(p)
  }
}
impl WriteFileTool {
  fn resolve(&self, name: &str) -> Result<std::path::PathBuf, String> {
    let p = self.home.join("files").join(name);
    if !p.starts_with(&self.home) { return Err("Path escapes agent home.".into()); }
    Ok(p)
  }
}

#[async_trait]
impl AgentTool for ReadFileTool {
  fn name(&self) -> String { "read_file".into() }
  fn description(&self) -> String { "Read a file from the agent's isolated files directory.".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "name": { "type": "string", "description": "File name relative to the agent files directory" } }, "required": ["name"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let name = str_arg(args, "name").ok_or("read_file requires a 'name' argument.")?;
    let path = self.resolve(&name)?;
    if !path.exists() { return Ok(format!("File '{}' does not exist in the agent files directory.", name)); }
    Ok(clip(&fs::read_to_string(&path).map_err(|e| e.to_string())?))
  }
}

#[async_trait]
impl AgentTool for WriteFileTool {
  fn name(&self) -> String { "write_file".into() }
  fn description(&self) -> String { "Write text content to a file in the agent's isolated files directory.".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "name": { "type": "string", "description": "File name relative to the agent files directory" }, "content": { "type": "string", "description": "Full text content to write" } }, "required": ["name", "content"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let name = str_arg(args, "name").ok_or("write_file requires a 'name' argument.")?;
    let content = str_arg(args, "content").ok_or("write_file requires a 'content' argument.")?;
    let path = self.resolve(&name)?;
    if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    fs::write(&path, &content).map_err(|e| e.to_string())?;
    Ok(format!("Wrote {} bytes to {}", content.len(), name))
  }
}

// Host-filesystem tools (gated behind the agent's `host_fs` permission).
// These deliberately escape the per-agent sandbox â€” treat as powerful.

pub(crate) struct SearchFilesTool;   // recursive filename search from a root path
pub(crate) struct ReadAnyFileTool;   // read any file by absolute path (size-capped)
pub(crate) struct RunCommandTool;    // execute a shell command, return stdout+stderr

#[async_trait]
impl AgentTool for SearchFilesTool {
  fn name(&self) -> String { "search_files".into() }
  fn description(&self) -> String { "Recursively search a directory for files whose name contains a substring. Params: pattern (string, e.g. 'resume'), startPath (string, optional, e.g. 'A:/'), maxResults (integer, optional, default 20).".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "pattern": { "type": "string" }, "startPath": { "type": "string" }, "maxResults": { "type": "integer" } }, "required": ["pattern"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let pattern = str_arg(args, "pattern").ok_or("search_files requires a 'pattern' argument.")?;
    let start = str_arg(args, "startPath").unwrap_or_else(|| "A:/".into());
    let max = args.get("maxResults").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let root = std::path::Path::new(&start);
    if !root.exists() { return Ok(format!("Path '{start}' does not exist.")); }
    let lower = pattern.to_lowercase();
    let mut found: Vec<String> = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    let mut visited: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
    while let Some(dir) = dirs.pop() {
      if !visited.insert(dir.clone()) { continue; }
      let Ok(entries) = fs::read_dir(&dir) else { continue };
      for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
          dirs.push(path);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
          if name.to_lowercase().contains(&lower) {
            found.push(path.to_string_lossy().to_string());
            if found.len() >= max { return Ok(found.join("\n")); }
          }
        }
      }
    }
    Ok(if found.is_empty() { "No files found matching that name." .to_string() } else { found.join("\n") })
  }
}

#[async_trait]
impl AgentTool for ReadAnyFileTool {
  fn name(&self) -> String { "read_file_any".into() }
  fn description(&self) -> String { "Read a file by absolute path (anywhere on the device). Params: path (string), maxBytes (integer, optional, default 20000).".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "path": { "type": "string" }, "maxBytes": { "type": "integer" } }, "required": ["path"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let path = str_arg(args, "path").ok_or("read_file_any requires a 'path' argument.")?;
    let max = args.get("maxBytes").and_then(|v| v.as_u64()).unwrap_or(20000) as usize;
    let p = std::path::Path::new(&path);
    if !p.exists() { return Ok(format!("File '{path}' does not exist.")); }
    let bytes = fs::read(p).map_err(|e| e.to_string())?;
    if bytes.len() > max { return Ok(format!("File is {} bytes (over the {} byte cap). Showing the first {}:\n{}", bytes.len(), max, max, String::from_utf8_lossy(&bytes[..max]))); }
    Ok(String::from_utf8_lossy(&bytes).to_string())
  }
}

#[async_trait]
impl AgentTool for RunCommandTool {
  fn name(&self) -> String { "run_command".into() }
  fn description(&self) -> String { "Run a shell command on the host machine and return stdout + stderr. Params: command (string). Requires explicit user approval â€” the user confirms before it executes.".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "command": { "type": "string" } }, "required": ["command"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let command = str_arg(args, "command").ok_or("run_command requires a 'command' argument.")?;
    // Runs through the system shell; on Windows this is cmd /C.
    #[cfg(windows)] let output = std::process::Command::new("cmd").args(["/C", &command]).output();
    #[cfg(not(windows))] let output = std::process::Command::new("sh").args(["-c", &command]).output();
    let output = output.map_err(|e| format!("Failed to run command: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let status = output.status;
    let mut out = String::new();
    if !status.success() { out.push_str(&format!("Exit code: {}\n", status.code().unwrap_or(-1))); }
    out.push_str(&stdout);
    if !stderr.is_empty() { out.push_str(&format!("\n[stderr]\n{stderr}")); }
    Ok(clip(&out))
  }
}
