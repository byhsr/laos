// The `yt_dlp` tool — video/audio downloads through the yt-dlp CLI.
//
// yt-dlp is the de-facto downloader for YouTube and thousands of other hosts.
// Its CLI is the documented interface, so this wraps one invocation: save the
// video (merged to mp4) or extract the audio (mp3) into a folder the user can
// find. It shells out and writes to disk, so it rides on the `host_fs`
// permission alongside Monid and run_command.

use async_trait::async_trait;

use super::{str_arg, AgentTool};

pub(crate) struct YtDlpTool;

#[async_trait]
impl AgentTool for YtDlpTool {
  fn name(&self) -> String { "yt_dlp".into() }

  fn description(&self) -> String {
    "Download a video, or extract its audio, from a URL (YouTube and thousands of other sites) with the \
yt-dlp CLI. Params: url (string, required), mode ('video' | 'audio', default 'video'; 'audio' extracts mp3), \
outputDir (string, optional — absolute folder; defaults to the user's Downloads), filename (string, optional \
— base name without extension; default is the video's own title), playlist (boolean, default false — true \
downloads an entire playlist instead of the single video). Needs yt-dlp on PATH (and ffmpeg for the audio \
and merge steps); a long video can take a while.".into()
  }

  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": {
      "url": { "type": "string" },
      "mode": { "type": "string", "enum": ["video", "audio"] },
      "outputDir": { "type": "string" },
      "filename": { "type": "string" },
      "playlist": { "type": "boolean" }
    }, "required": ["url"] })
  }

  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let dir = output_dir(args);
    let line = shell_line(args, &dir)?;
    // Make the target folder first so the default (Downloads) works on a fresh
    // machine instead of failing on a missing directory.
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    tokio::task::spawn_blocking(move || crate::tools::run_shell_in(&line, "")).await.map_err(|e| e.to_string())?
  }
}

/// The full yt-dlp shell line for one call. Pure (no disk), so the flags can be
/// tested without the CLI installed.
fn shell_line(args: &serde_json::Value, dir: &std::path::Path) -> Result<String, String> {
  let url = str_arg(args, "url").unwrap_or_default();
  if url.trim().is_empty() { return Err("yt_dlp needs a 'url'.".into()); }
  let mode = mode_arg(args)?;
  let template = output_template(dir, str_arg(args, "filename").as_deref());
  let playlist = args.get("playlist").and_then(|v| v.as_bool()).unwrap_or(false);

  let mut line = String::from("yt-dlp --no-warnings --no-progress");
  line.push_str(&format!(" -o {}", quote(&template)?));
  match mode.as_str() {
    "audio" => line.push_str(" -x --audio-format mp3"),
    _ => line.push_str(" -f \"bv*+ba/b\" --merge-output-format mp4"),
  }
  if !playlist { line.push_str(" --no-playlist"); }
  line.push_str(&format!(" {}", quote(&url)?));
  Ok(line)
}

/// `video` (default) or `audio`, case-insensitive.
fn mode_arg(args: &serde_json::Value) -> Result<String, String> {
  let raw = str_arg(args, "mode").unwrap_or_default();
  let mode = raw.trim().to_lowercase();
  let mode = if mode.is_empty() { "video".to_string() } else { mode };
  match mode.as_str() {
    "video" | "audio" => Ok(mode),
    other => Err(format!("Unknown yt_dlp mode \"{other}\". Use video | audio.")),
  }
}

/// Where the download lands: an explicit `outputDir`, else the user's Downloads.
fn output_dir(args: &serde_json::Value) -> std::path::PathBuf {
  match str_arg(args, "outputDir") {
    Some(d) if !d.trim().is_empty() => std::path::PathBuf::from(d.trim()),
    _ => {
      // Windows-first, like the rest of the app; HOME covers the rest.
      match std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        Ok(home) => std::path::Path::new(&home).join("Downloads"),
        Err(_) => std::env::temp_dir(),
      }
    }
  }
}

/// The `-o` template: a named base file, or the site's own title.
fn output_template(dir: &std::path::Path, filename: Option<&str>) -> String {
  let dir = dir.to_string_lossy().trim_end_matches(|c| c == '/' || c == '\\').to_string();
  match filename.map(str::trim).filter(|f| !f.is_empty()) {
    Some(name) => format!("{dir}/{name}.%(ext)s"),
    None => format!("{dir}/%(title)s.%(ext)s"),
  }
}

/// A double-quoted shell argument; rejects the two characters that would break
/// out of the quotes.
fn quote(s: &str) -> Result<String, String> {
  if s.contains('"') || s.contains('\n') {
    return Err("drop double quotes and newlines from this value.".into());
  }
  Ok(format!("\"{}\"", s.trim()))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn defaults_to_video_full_title_into_the_given_dir() {
    let a = serde_json::json!({ "url": "https://youtu.be/abc" });
    let line = shell_line(&a, std::path::Path::new("C:/dl")).unwrap();
    assert_eq!(
      line,
      "yt-dlp --no-warnings --no-progress -o \"C:/dl/%(title)s.%(ext)s\" -f \"bv*+ba/b\" \
--merge-output-format mp4 --no-playlist \"https://youtu.be/abc\""
    );
  }

  #[test]
  fn audio_mode_extracts_mp3_to_a_named_file() {
    let a = serde_json::json!({ "url": "https://youtu.be/abc", "mode": "audio", "filename": "song" });
    let line = shell_line(&a, std::path::Path::new("C:/dl/")).unwrap();
    assert!(line.contains("-o \"C:/dl/song.%(ext)s\""), "{line}");
    assert!(line.contains("-x --audio-format mp3"), "{line}");
    assert!(!line.contains("--merge-output-format"), "{line}");
  }

  #[test]
  fn playlist_true_drops_the_no_playlist_flag() {
    let a = serde_json::json!({ "url": "https://youtu.be/abc", "playlist": true });
    assert!(!shell_line(&a, std::path::Path::new("C:/dl")).unwrap().contains("--no-playlist"));
  }

  #[test]
  fn rejects_a_missing_url_a_bad_mode_and_embedded_quotes() {
    assert!(shell_line(&serde_json::json!({}), std::path::Path::new("C:/dl")).is_err());
    assert!(shell_line(&serde_json::json!({ "url": "u", "mode": "gif" }), std::path::Path::new("C:/dl")).is_err());
    assert!(shell_line(&serde_json::json!({ "url": "a\"b" }), std::path::Path::new("C:/dl")).is_err());
  }
}
