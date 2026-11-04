// Routing — where a captured thing belongs.
//
// The prose in `ROUTING_RULES` is the contract the model is given; the code here
// is the deterministic reading of the same contract. A turn that clearly states a
// standing rule or a timed reminder is filed immediately, so it is never lost to
// tool-choice drift; anything ambiguous is left to the model (or, when the message
// looks like content worth filing, read by the classifier role).

use std::sync::OnceLock;

use regex::Regex;
use rusqlite::Connection;
use tauri::AppHandle;

/// The contract, injected into every agent and Manager prompt.
pub(crate) const ROUTING_RULES: &str = "\nWhere things go (do not mix these up):\n\
- items — anything with a when or a do: notes, todos, reminders, goals, resources and habits. \
Use the items tool ('add' to record, 'list' to show, 'update' to change, 'complete' to tick off, 'delete' to remove). Reminders are scheduled and pushed to Telegram.\n\
- remember — durable facts and standing RULES about the user (\"never use emoji or em dashes\", \
\"prefers IST\", \"my name is Ranas\"). kind='policy' for a never/always rule, 'preference' for a taste, \
'semantic' for a plain fact. These are injected into every future prompt.\n\
- knowledge_base — shared REFERENCE material any agent may need later: company wiki, ICP, specs, \
policies, runbooks, product facts. NOT for tasks, reminders or personal rules.";

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Destination { Memory, KnowledgeBase, Items }

/// What a turn was read as, and what to tell the model about it so it doesn't
/// file the same thing twice.
pub(crate) struct Triage {
  /// Appended to the system prompt.
  pub note: String,
  /// A short status step for the chat bubble.
  pub step: String,
}

// Cues deliberately phrase-level: a false positive writes a memory row, but a
// false negative means the user's rule is silently ignored.
const NEGATIVE_RULE: &[&str] = &[
  "never use", "never say", "never do", "never write", "never respond", "never reply", "never mention",
  "don't ever", "dont ever", "do not ever", "don't use", "dont use", "stop using", "stop saying",
  "avoid using", "no emoji", "no em-dash", "no em dash",
];
const POSITIVE_RULE: &[&str] = &[
  "always use", "always answer", "always reply", "always write", "always respond", "always remember",
  "from now on", "going forward", "every time you ", "make sure you always",
];
const REMINDER_CUE: &[&str] = &["remind me", "set a reminder", "reminder to", "remind ", "ping me", "nudge me"];
const TODO_CUE: &[&str] = &[
  "i need to", "i have to", "i must ", "add a task", "add a todo", "todo:", "to-do:",
  "add to my list", "put on my list",
];
/// The only thing that justifies a classifier round-trip: a message that looks
/// like content wanting to be filed, whose destination isn't clear from the cues.
const FILING_HINTS: &[&str] = &[
  "remind", "remember", "don't forget", "dont forget", "keep track", "task", "todo", "to-do",
  "deadline", "due ", "never ", "always ", "from now on", "note that",
  // The other item kinds, so a goal / link / habit / note reaches the classifier too.
  "my goal", "goal is", "i promise", "i commit", "save this link", "save the link",
  "bookmark", "make a note", "jot down", "every day", "every week", "every morning", "habit",
];
/// A first-person habit ("I never use Spotify") is a fact about the user, not an
/// instruction to the assistant — it must not become a policy.
const ABOUT_SELF: &[&str] = &[
  "i never use", "i dont use", "i don't use", "i always use", "i used to", "we never use", "we always use",
];

fn clock_re() -> &'static Regex {
  static RE: OnceLock<Regex> = OnceLock::new();
  RE.get_or_init(|| Regex::new(r"(?i)\b(\d{1,2})(?::(\d{2}))?\s*(am|pm)\b|\b(\d{1,2}):(\d{2})\b").unwrap())
}

/// A clock time in the text, as (hour, minute) on a 24-hour clock.
fn clock_in(text: &str) -> Option<(u32, u32)> {
  let cap = clock_re().captures(text)?;
  if let Some(h) = cap.get(1) {
    let mut hour: u32 = h.as_str().parse().ok()?;
    let minute: u32 = cap.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    let meridiem = cap.get(3).map(|a| a.as_str().to_ascii_lowercase()).unwrap_or_default();
    if meridiem == "pm" && hour < 12 { hour += 12; }
    if meridiem == "am" && hour == 12 { hour = 0; }
    return Some((hour, minute));
  }
  Some((cap.get(4)?.as_str().parse().ok()?, cap.get(5)?.as_str().parse().ok()?))
}

/// 'policy' for a never/always rule, 'preference' for a taste, 'semantic' for a
/// plain fact. Shared with the explicit-remember fast path.
pub(crate) fn rule_kind(text: &str) -> &'static str {
  let t = text.to_lowercase();
  if NEGATIVE_RULE.iter().any(|c| t.contains(c)) { return "policy"; }
  if POSITIVE_RULE.iter().any(|c| t.contains(c)) { return "preference"; }
  "semantic"
}

/// Resolves a free-text due ("tomorrow 9am", "at 17:30") to an absolute instant in
/// the configured zone. An ISO-8601 stamp passes straight through.
pub(crate) fn resolve_due(conn: &Connection, text: &str) -> Option<String> {
  let text = text.trim();
  if text.is_empty() { return None; }
  if chrono::DateTime::parse_from_rfc3339(text).is_ok() { return Some(text.to_string()); }
  let low = text.to_lowercase();
  let (hour, minute) = clock_in(&low)?;
  let day = if low.contains("tomorrow") { 1 } else if low.contains("next week") { 7 } else { 0 };
  crate::tz::resolve_clock(conn, hour, minute, day)
}

/// The deterministic reading: Some when the cues are unambiguous, None when the
/// turn is better judged by the model or the classifier.
pub(crate) fn route(text: &str) -> Option<(Destination, &'static str)> {
  let t = text.to_lowercase();
  let about_self = ABOUT_SELF.iter().any(|c| t.contains(c));
  if !about_self && (NEGATIVE_RULE.iter().any(|c| t.contains(c)) || POSITIVE_RULE.iter().any(|c| t.contains(c))) {
    return Some((Destination::Memory, rule_kind(text)));
  }
  // A reminder needs a time; "remind me" with no when is a message, not a schedule.
  if REMINDER_CUE.iter().any(|c| t.contains(c)) && clock_in(&t).is_some() {
    return Some((Destination::Items, "reminder"));
  }
  if TODO_CUE.iter().any(|c| t.contains(c)) {
    return Some((Destination::Items, "todo"));
  }
  None
}

/// Reads a turn and files it where it belongs. Returns None when there's nothing
/// to file — the model's own tools still cover those turns.
///
/// Takes the app handle rather than a connection: the classifier path awaits, and
/// a `&Connection` held across an await would make every caller's future non-Send.
pub(crate) async fn triage(
  app: &AppHandle, agent_id: &str, text: &str, project: Option<&str>,
) -> Option<Triage> {
  let text = text.trim();
  if text.chars().count() < 8 { return None; }

  // Decide first — this is the only part that awaits.
  let (destination, kind) = match route(text) {
    Some((d, k)) => (d, k.to_string()),
    None => {
      // Only pay for the classifier when the message actually looks like content
      // wanting to be filed, so an ordinary chat turn costs nothing extra.
      if !FILING_HINTS.iter().any(|c| text.to_lowercase().contains(c)) { return None; }
      let (d, k, _) = classifier_route(app, text).await?;
      (d, k)
    }
  };

  // Everything below is synchronous, so the connection is never held across an
  // await point.
  let conn = crate::db::db(app).ok()?;
  let due = if destination == Destination::Items { resolve_due(&conn, text) } else { None };
  match destination {
    Destination::Items => {
      let (tags, collection) = crate::capture::tags_in(text);
      let url = crate::capture::url_in(text);
      let title = crate::capture::title_from(text);
      let item = crate::capture::add(&conn, crate::capture::NewItem {
        kind: &kind, title: &title, body: text, url: &url, tags, collection: &collection,
        due_at: due.clone(), cadence: "", project_id: project.unwrap_or(""), source: "app", owner: "",
      }).ok()?;
      crate::brain::ingest_item(app, &conn, &item, &crate::capture::synthesized_class(&kind), "life");
      let when = due.as_deref().map(|d| format!(" for {}", crate::tz::format(&conn, d))).unwrap_or_default();
      Some(Triage {
        step: format!("filed as {kind}{when}"),
        note: format!("\nThis message was already filed as a {kind}{when} in Items (id: {}) — do not create it again.", item.id),
      })
    }
    Destination::Memory => {
      let title = crate::capture::title_from(text);
      let (_, created) = crate::memory::store_rule(app, agent_id, &kind, &title, text).ok()?;
      Some(Triage {
        step: format!("saved as a standing {kind}"),
        note: format!(
          "\nThis message was already saved as a standing {kind} in memory{} — it applies to every future chat. Do not store it again.",
          if created { "" } else { " (it was already there)" }
        ),
      })
    }
    Destination::KnowledgeBase => None,
  }
}

/// The reading the re-file pass uses: the cues first, then the classifier. Only
/// destinations that actually move a doc are returned.
pub(crate) async fn classify_text(app: &AppHandle, text: &str) -> Option<(Destination, String)> {
  if let Some((d, k)) = route(text) {
    return if d == Destination::KnowledgeBase { None } else { Some((d, k.to_string())) };
  }
  let (d, k, _) = classifier_route(app, text).await?;
  Some((d, k))
}

/// The classifier's reading, used only for turns that look like content but have
/// no clear cue. Only content types with an obvious destination are filed.
async fn classifier_route(app: &AppHandle, text: &str) -> Option<(Destination, String, Option<String>)> {
  let class = crate::classify::classify(app, text, "").await;
  let destination = match class.content_type.as_str() {
    // Every item kind lands in items — not just a task: a captured note, idea,
    // goal or dated event belongs there too, so it isn't lost to chat.
    "task" | "reminder" | "habit" | "goal" | "note" | "idea" | "event" => Destination::Items,
    "preference" => Destination::Memory,
    _ => return None,
  };
  let kind = if crate::capture::KINDS.contains(&class.feature.trim()) {
    class.feature.trim().to_string()
  } else {
    match class.content_type.as_str() {
      "goal" => "goal".to_string(),
      "event" | "reminder" => "reminder".to_string(),
      "preference" => "preference".to_string(),
      _ => "note".to_string(),
    }
  };
  let due = class.params.due_at.filter(|d| !d.trim().is_empty());
  Some((destination, kind, due))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn dest(text: &str) -> Option<Destination> { route(text).map(|(d, _)| d) }

  /// A standing rule about the user is memory, and a negative one is a policy.
  #[test]
  fn a_never_rule_routes_to_memory_as_a_policy() {
    assert_eq!(dest("never use emoji or em dashes"), Some(Destination::Memory));
    assert_eq!(route("never use emoji or em dashes").unwrap().1, "policy");
  }

  /// A reminder needs a time, and lands in items.
  #[test]
  fn a_timed_reminder_routes_to_items() {
    assert_eq!(dest("remind me to call the bank at 5pm"), Some(Destination::Items));
    assert_eq!(route("remind me to call the bank at 5pm").unwrap().1, "reminder");
    assert_eq!(dest("remind me to call the bank tomorrow at 09:30"), Some(Destination::Items));
  }

  /// "remind me" with no when is a message, not a schedule — the model decides.
  #[test]
  fn a_timeless_reminder_is_not_routed() {
    assert_eq!(dest("remind me what we decided about pricing"), None);
  }

  /// An ordinary question isn't routed at all, so no extra model call is spent.
  #[test]
  fn a_chat_turn_is_not_routed() {
    assert_eq!(dest("what does the retrieval pipeline do?"), None);
  }

  /// A todo without a time still belongs in items.
  #[test]
  fn a_todo_routes_to_items() {
    assert_eq!(dest("i need to renew the domain"), Some(Destination::Items));
  }

  /// A habit stated about the user is a fact, not an instruction to the assistant.
  #[test]
  fn a_first_person_habit_is_not_a_rule() {
    assert_eq!(dest("i never use spotify"), None);
  }
}
