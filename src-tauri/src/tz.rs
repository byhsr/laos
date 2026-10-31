// Timezone. The zone the user picks (Settings → you & your assistant) is what
// every due date, reminder and relative time is resolved in. It defaults to IST,
// so an unconfigured install still schedules against the user's own clock rather
// than UTC.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::Connection;

/// IST — the default when nothing is configured.
pub(crate) const DEFAULT_TZ: &str = "Asia/Kolkata";

fn fallback() -> Tz { DEFAULT_TZ.parse().unwrap_or(chrono_tz::UTC) }

/// The configured zone, falling back to IST — and to UTC for an unparsable value,
/// so a typo in Settings can never break scheduling.
pub(crate) fn zone(conn: &Connection) -> Tz {
  let configured = crate::app_settings::identity(conn).timezone;
  let configured = configured.trim().to_string();
  if configured.is_empty() { return fallback(); }
  configured.parse().unwrap_or_else(|_| fallback())
}

/// `HH:MM` on today + `day_offset` in the configured zone. On day 0 a slot that
/// has already passed rolls to tomorrow, so "at 9" said at 10pm means tomorrow.
pub(crate) fn resolve_clock(conn: &Connection, hour: u32, minute: u32, day_offset: i64) -> Option<String> {
  if hour > 23 || minute > 59 { return None; }
  let z = zone(conn);
  let now = Utc::now().with_timezone(&z);
  let mut date = now.date_naive() + Duration::days(day_offset);
  let mut naive = date.and_hms_opt(hour, minute, 0)?;
  if day_offset == 0 && naive <= now.naive_local() {
    date += Duration::days(1);
    naive = date.and_hms_opt(hour, minute, 0)?;
  }
  z.from_local_datetime(&naive).single().map(|d| d.to_rfc3339())
}

/// `YYYY-MM-DD` + `HH:MM` in the configured zone.
pub(crate) fn resolve_date_time(conn: &Connection, date: &str, time: &str) -> Option<String> {
  let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
  let (h, m) = time.split_once(':')?;
  let naive = d.and_hms_opt(h.trim().parse().ok()?, m.trim().parse().ok()?, 0)?;
  zone(conn).from_local_datetime(&naive).single().map(|x| x.to_rfc3339())
}

/// Renders an RFC3339 instant in the configured zone ("Nov 01 09:00").
pub(crate) fn format(conn: &Connection, iso: &str) -> String {
  DateTime::parse_from_rfc3339(iso)
    .map(|d| d.with_timezone(&zone(conn)).format("%b %d %H:%M").to_string())
    .unwrap_or_else(|_| iso.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::Timelike;

  fn db_with_tz(tz: &str) -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);").unwrap();
    conn.execute(
      "INSERT INTO settings (key, value) VALUES ('identity', ?1)",
      rusqlite::params![format!("{{\"timezone\":\"{tz}\"}}")],
    ).unwrap();
    conn
  }

  /// An unset timezone must resolve to IST, not the machine's zone.
  #[test]
  fn an_unset_timezone_falls_back_to_ist() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);").unwrap();
    assert_eq!(zone(&conn).name(), DEFAULT_TZ);
  }

  /// The same wall clock means a different instant in each zone. Asserted as a
  /// round-trip rather than a fixed delta, so a DST boundary can't break it.
  #[test]
  fn a_clock_resolves_in_the_configured_zone() {
    let ist = db_with_tz("Asia/Kolkata");
    let ny = db_with_tz("America/New_York");
    for (conn, name) in [(&ist, "Asia/Kolkata"), (&ny, "America/New_York")] {
      let iso = resolve_clock(conn, 9, 0, 3).unwrap();
      let tz: Tz = name.parse().unwrap();
      let local = DateTime::parse_from_rfc3339(&iso).unwrap().with_timezone(&tz);
      assert_eq!((local.hour(), local.minute()), (9, 0), "{name} should read back as 09:00");
    }
    assert_ne!(resolve_clock(&ist, 9, 0, 3), resolve_clock(&ny, 9, 0, 3));
  }

  /// A slot already past rolls to tomorrow rather than firing immediately.
  #[test]
  fn a_past_slot_rolls_forward() {
    let conn = db_with_tz(DEFAULT_TZ);
    let now = zone(&conn).from_utc_datetime(&Utc::now().naive_utc());
    let past = (now.hour() + 23) % 24;
    let resolved = resolve_clock(&conn, past, 0, 0).unwrap();
    assert!(DateTime::parse_from_rfc3339(&resolved).unwrap() > Utc::now().fixed_offset());
  }
}
