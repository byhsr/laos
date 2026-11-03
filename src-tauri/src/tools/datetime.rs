// The `current_datetime` tool — every agent's clock.
//
// Nothing else tells a model what "now" is: tz.rs only *formats* stored times,
// and no prompt carries the current date. So an agent asked to schedule
// "tomorrow" had to guess the day. This reads the user's configured zone
// (default IST) and reports now plus the surrounding days.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use tauri::AppHandle;

use super::AgentTool;

pub(crate) struct CurrentTimeTool {
  pub(crate) app: AppHandle,
}

#[async_trait]
impl AgentTool for CurrentTimeTool {
  fn name(&self) -> String { "current_datetime".into() }

  fn description(&self) -> String {
    "Get the current date and time in the user's configured timezone. Use this whenever a task depends on \
'now' — scheduling a reminder, resolving 'tomorrow' / 'next week', or reasoning about today's date. Returns \
the current instant, the weekday, the zone name, and tomorrow and yesterday. No arguments.".into()
  }

  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": {}, "required": [] })
  }

  async fn run(&self, _args: &serde_json::Value) -> Result<String, String> {
    let conn = crate::db::db(&self.app)?;
    let tz = crate::tz::zone(&conn);
    Ok(render(&tz, Utc::now()))
  }
}

fn line(label: &str, dt: &DateTime<Tz>, zone: &str) -> String {
  format!("{label}: {} ({}, {} · {zone})", dt.to_rfc3339(), dt.format("%A"), dt.format("%b %d %Y, %I:%M %p"))
}

/// Now plus the surrounding days in `tz`. Split out so the format can be tested
/// against a fixed instant instead of the wall clock.
fn render(tz: &Tz, now: DateTime<Utc>) -> String {
  let zone = tz.name();
  let line_for = |offset: i64| {
    let label = match offset {
      -1 => "Yesterday",
      1 => "Tomorrow",
      _ => "Now",
    };
    line(label, &(now + Duration::days(offset)).with_timezone(tz), zone)
  };
  format!("{}\n{}\n{}", line_for(0), line_for(1), line_for(-1))
}

/// The tool every agent (and the Manager) holds.
pub(crate) fn current_time_tool(app: &AppHandle) -> Box<dyn AgentTool> {
  Box::new(CurrentTimeTool { app: app.clone() })
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::TimeZone;

  /// The rendered clock is the configured zone's, not UTC — and the neighbouring
  /// days are that zone's days too.
  #[test]
  fn renders_now_and_neighbouring_days_in_the_zone() {
    let tz: Tz = "Asia/Kolkata".parse().unwrap();
    let now = Utc.with_ymd_and_hms(2026, 11, 3, 8, 52, 0).unwrap();
    let out = render(&tz, now);
    assert!(out.contains("Now: 2026-11-03T14:22:00+05:30"), "{out}");
    assert!(out.contains("Tomorrow: 2026-11-04T14:22:00+05:30"), "{out}");
    assert!(out.contains("Yesterday: 2026-11-02T14:22:00+05:30"), "{out}");
    assert!(out.contains("Asia/Kolkata"), "{out}");
  }
}
