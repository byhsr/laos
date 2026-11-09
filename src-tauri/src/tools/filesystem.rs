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
pub(crate) struct WriteAnyFileTool;  // create/overwrite a file at an absolute path
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

// Writes a file at an absolute path anywhere on the device — the write half of
// read_file_any. Agents produce markdown, CSV and the like, and the sandboxed
// write_file can only reach the agent's own folder; this reaches wherever the
// user actually wants the artifact. Parent folders are created, and `append`
// adds to an existing file instead of replacing it.
#[async_trait]
impl AgentTool for WriteAnyFileTool {
  fn name(&self) -> String { "write_file_any".into() }
  fn description(&self) -> String { "Create or overwrite a text file at an absolute path (anywhere on the device) — markdown, CSV, notes, code. Params: path (string), content (string), append (boolean, optional, default false — adds to an existing file). Parent folders are created. Requires explicit user approval.".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "path": { "type": "string", "description": "Absolute path of the file to write" }, "content": { "type": "string", "description": "Full text content to write" }, "append": { "type": "boolean", "description": "Append instead of overwriting" } }, "required": ["path", "content"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let path = str_arg(args, "path").ok_or("write_file_any requires a 'path' argument.")?;
    if path.trim().is_empty() { return Err("write_file_any needs a non-empty 'path'.".into()); }
    let content = str_arg(args, "content").ok_or("write_file_any requires a 'content' argument.")?;
    let append = args.get("append").and_then(|v| v.as_bool()).unwrap_or(false);
    let p = std::path::Path::new(&path);
    if let Some(parent) = p.parent() {
      if !parent.as_os_str().is_empty() { fs::create_dir_all(parent).map_err(|e| format!("Could not create '{}': {e}", parent.display()))?; }
    }
    let bytes = content.as_bytes().len();
    if append {
      use std::io::Write;
      let mut file = fs::OpenOptions::new().create(true).append(true).open(p).map_err(|e| e.to_string())?;
      file.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
      Ok(format!("Appended {bytes} bytes to {path}"))
    } else {
      fs::write(p, &content).map_err(|e| e.to_string())?;
      Ok(format!("Wrote {bytes} bytes to {path}"))
    }
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
    run_shell(&command)
  }
}

// Executes a shell command and returns stdout + stderr (clipped). Shared by the
// run_command tool and the workflow script node.
pub(crate) fn run_shell(command: &str) -> Result<String, String> {
  run_shell_in(command, "")
}

// Like run_shell, but runs in `cwd` when it is non-empty (saved scripts).
pub(crate) fn run_shell_in(command: &str, cwd: &str) -> Result<String, String> {
  // Runs through the system shell; on Windows this is cmd /C. The app is a GUI
  // process, so CREATE_NO_WINDOW keeps it from flashing a console window.
  #[cfg(windows)]
  let output = {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut c = std::process::Command::new("cmd");
    c.args(["/C", command]);
    if !cwd.is_empty() { c.current_dir(cwd); }
    c.creation_flags(CREATE_NO_WINDOW).output()
  };
  #[cfg(not(windows))]
  let output = {
    let mut c = std::process::Command::new("sh");
    c.args(["-c", command]);
    if !cwd.is_empty() { c.current_dir(cwd); }
    c.output()
  };
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

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn write_file_any_creates_then_appends() {
    let dir = std::env::temp_dir().join(format!("lup-wfa-{}", std::process::id()));
    let path = dir.join("nested/out.md");
    let p = path.to_string_lossy().to_string();
    let tool = WriteAnyFileTool;
    let first = tool.run(&serde_json::json!({ "path": p.clone(), "content": "# hi\n" })).await.unwrap();
    assert!(first.contains("Wrote"), "reports the write: {first}");
    tool.run(&serde_json::json!({ "path": p.clone(), "content": "more\n", "append": true })).await.unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "# hi\nmore\n");
    let _ = fs::remove_dir_all(&dir);
  }
}
