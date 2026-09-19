// Text-protocol tool calls.
//
// Some models (DeepSeek V3.1 and friends) emit tool calls as *text* using their
// special-token markup instead of the provider's native `tool_calls` field:
//
//   <｜DSML｜tool_calls><｜DSML｜invoke name="delegate_task">
//     <｜DSML｜parameter name="agentId" string="true">agent-1</｜DSML｜parameter>
//   </｜DSML｜invoke></｜DSML｜tool_calls>
//
// Left alone the call is silently ignored *and* the raw markup streams into the
// chat as if it were the reply. `parse_text_tool_calls` recovers the call so it
// can actually be executed; `ToolCallLeakFilter` keeps the markup out of the
// visible stream.

const DSML_OPEN: &str = "<\u{FF5C}DSML\u{FF5C}";
const DSML_CLOSE: &str = "</\u{FF5C}DSML\u{FF5C}";

// Markers that open a text-protocol call, and the ones that close the whole block.
const OPEN_MARKERS: [&str; 4] = [DSML_OPEN, "<|DSML|", "<tool_calls>", "<tool\u{2581}calls>"];
const CLOSE_MARKERS: [&str; 4] = [
  "</\u{FF5C}DSML\u{FF5C}tool_calls>",
  "</|DSML|tool_calls>",
  "</tool_calls>",
  "</tool\u{2581}calls>",
];
// Longest marker above; used to hold back a tail that could still complete one.
const MAX_MARKER: usize = 24;

#[derive(Debug)]
pub(crate) struct TextCall {
  pub(crate) name: String,
  pub(crate) args: serde_json::Value,
}

// `<｜DSML｜tag …>` / `<|DSML|tag …>` → `<tag …>`, so the markup parses as plain
// XML-ish tags. Text without a delimiter is returned untouched.
fn normalize(content: &str) -> String {
  content
    .replace(DSML_CLOSE, "</")
    .replace(DSML_OPEN, "<")
    .replace("</|DSML|", "</")
    .replace("<|DSML|", "<")
}

// Reads `key="value"` (single-quoted and bare values too) out of an opening tag.
fn attr(tag: &str, key: &str) -> Option<String> {
  let needle = format!("{key}=");
  let at = tag.find(&needle)? + needle.len();
  let rest = &tag[at..];
  let quote = rest.chars().next()?;
  if quote == '"' || quote == '\'' {
    let end = rest[1..].find(quote)? + 1;
    Some(rest[1..end].to_string())
  } else {
    let end = rest.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(rest.len());
    Some(rest[..end].to_string())
  }
}

// `<parameter name="k" string="true">v</parameter>` → { k: "v" }. `string="false"`
// is parsed as JSON so numbers/booleans/objects keep their type.
fn parse_params(body: &str) -> serde_json::Map<String, serde_json::Value> {
  let mut map = serde_json::Map::new();
  let mut cursor = 0usize;
  while let Some(rel) = body[cursor..].find("<parameter") {
    let start = cursor + rel;
    let Some(open_end_rel) = body[start..].find('>') else { break };
    let open_end = start + open_end_rel;
    let open_tag = &body[start..=open_end];
    let name = attr(open_tag, "name").unwrap_or_default();
    let is_string = attr(open_tag, "string").map(|v| v != "false").unwrap_or(true);
    let Some(close_rel) = body[open_end + 1..].find("</parameter>") else { break };
    let raw = &body[open_end + 1..open_end + 1 + close_rel];
    if !name.is_empty() {
      let value = if is_string {
        serde_json::Value::String(raw.to_string())
      } else {
        serde_json::from_str(raw.trim()).unwrap_or_else(|_| serde_json::Value::String(raw.to_string()))
      };
      map.insert(name, value);
    }
    cursor = open_end + 1 + close_rel + "</parameter>".len();
  }
  map
}

// Recovers text-protocol calls from a model reply. Returns the reply with the
// markup stripped (for display) plus the parsed calls. When nothing is found the
// original content is returned unchanged, so normal replies are untouched.
pub(crate) fn parse_text_tool_calls(content: &str) -> (String, Vec<TextCall>) {
  if !content.contains("DSML") && !content.contains("<tool") && !content.contains("<invoke") {
    return (content.to_string(), Vec::new());
  }
  let normalized = normalize(content);
  let mut calls = Vec::new();
  let mut spans: Vec<(usize, usize)> = Vec::new();
  let mut cursor = 0usize;
  while let Some(rel) = normalized[cursor..].find("<invoke") {
    let start = cursor + rel;
    let Some(open_end_rel) = normalized[start..].find('>') else { break };
    let open_end = start + open_end_rel;
    let name = attr(&normalized[start..=open_end], "name").unwrap_or_default();
    let Some(close_rel) = normalized[open_end + 1..].find("</invoke>") else { break };
    let body_start = open_end + 1;
    let body_end = body_start + close_rel;
    if !name.is_empty() {
      calls.push(TextCall { name, args: serde_json::Value::Object(parse_params(&normalized[body_start..body_end])) });
    }
    spans.push((start, body_end + "</invoke>".len()));
    cursor = body_end + "</invoke>".len();
  }
  if calls.is_empty() {
    return (content.to_string(), Vec::new());
  }
  let mut cleaned = normalized;
  for (start, end) in spans.iter().rev() {
    cleaned.replace_range(*start..*end, "");
  }
  for tag in ["<tool_calls>", "</tool_calls>", "<tool\u{2581}calls>", "</tool\u{2581}calls>"] {
    cleaned = cleaned.replace(tag, "");
  }
  (cleaned.trim().to_string(), calls)
}

// Earliest of `markers` in `hay`, as (index, marker length).
fn find_marker(hay: &str, markers: &[&str]) -> Option<(usize, usize)> {
  let mut best: Option<(usize, usize)> = None;
  for marker in markers {
    if let Some(at) = hay.find(marker) {
      if best.map(|(b, _)| at < b).unwrap_or(true) {
        best = Some((at, marker.len()));
      }
    }
  }
  best
}

// Length of the tail of `s` that could still grow into a marker (0 if none), so a
// marker split across deltas is never emitted half-way.
fn partial_marker_tail(s: &str) -> usize {
  let max = s.len().min(MAX_MARKER);
  for len in (1..=max).rev() {
    let start = s.len() - len;
    if !s.is_char_boundary(start) { continue; }
    let tail = &s[start..];
    if OPEN_MARKERS.iter().any(|m| m.starts_with(tail)) {
      return len;
    }
  }
  0
}

// A streaming guard: suppresses anything between an opening marker and the close
// of the call block, so model tool-call markup can never reach the chat bubble.
pub(crate) struct ToolCallLeakFilter {
  carry: String,
  suppressing: bool,
}

impl ToolCallLeakFilter {
  pub(crate) fn new() -> Self {
    Self { carry: String::new(), suppressing: false }
  }

  // Returns the part of `delta` that is safe to show.
  pub(crate) fn feed(&mut self, delta: &str) -> String {
    self.carry.push_str(delta);
    let mut out = String::new();
    loop {
      if self.suppressing {
        if let Some((at, len)) = find_marker(&self.carry, &CLOSE_MARKERS) {
          self.carry.drain(..at + len);
          self.suppressing = false;
          continue;
        }
        // Keep only enough tail to still recognise a split close marker.
        if self.carry.len() > MAX_MARKER {
          let mut cut = self.carry.len() - MAX_MARKER;
          while cut < self.carry.len() && !self.carry.is_char_boundary(cut) { cut += 1; }
          self.carry.drain(..cut);
        }
        break;
      }
      if let Some((at, _)) = find_marker(&self.carry, &OPEN_MARKERS) {
        out.push_str(&self.carry[..at]);
        self.carry.drain(..at);
        self.suppressing = true;
        continue;
      }
      let keep = partial_marker_tail(&self.carry);
      let emit = self.carry.len() - keep;
      out.push_str(&self.carry[..emit]);
      self.carry.drain(..emit);
      break;
    }
    out
  }

  // Anything still held at end-of-stream. A suppressed (never-closed) block is
  // dropped rather than shown.
  pub(crate) fn finish(&mut self) -> String {
    let tail = if self.suppressing { String::new() } else { std::mem::take(&mut self.carry) };
    self.carry.clear();
    tail
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn open(tag: &str) -> String { format!("<\u{FF5C}DSML\u{FF5C}{tag}>") }
  fn close(tag: &str) -> String { format!("</\u{FF5C}DSML\u{FF5C}{tag}>") }

  fn sample() -> String {
    let mut s = String::from("Let me try.\n");
    s.push_str(&open("tool_calls"));
    s.push_str(&open("invoke name=\"delegate_task\""));
    s.push_str(&open("parameter name=\"agentId\" string=\"true\""));
    s.push_str("agent-1");
    s.push_str(&close("parameter"));
    s.push_str(&open("parameter name=\"input\" string=\"true\""));
    s.push_str("Do the thing");
    s.push_str(&close("parameter"));
    s.push_str(&close("invoke"));
    s.push_str(&close("tool_calls"));
    s
  }

  #[test]
  fn parses_dsml_call_and_strips_markup() {
    let (cleaned, calls) = parse_text_tool_calls(&sample());
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "delegate_task");
    assert_eq!(calls[0].args["agentId"], "agent-1");
    assert_eq!(calls[0].args["input"], "Do the thing");
    assert_eq!(cleaned, "Let me try.");
  }

  #[test]
  fn plain_replies_are_untouched() {
    let (cleaned, calls) = parse_text_tool_calls("just a normal reply");
    assert!(calls.is_empty());
    assert_eq!(cleaned, "just a normal reply");
  }

  #[test]
  fn filter_never_shows_markup() {
    let mut f = ToolCallLeakFilter::new();
    let mut visible = String::new();
    visible.push_str(&f.feed("Here you "));
    visible.push_str(&f.feed(&format!("go {}", open("tool_calls"))));
    visible.push_str(&f.feed(&format!("{}{}", open("invoke name=\"x\""), open("parameter name=\"a\" string=\"true\""))));
    visible.push_str(&f.feed("1"));
    visible.push_str(&f.feed(&close("parameter")));
    visible.push_str(&f.feed(&format!("{}{}!", close("invoke"), close("tool_calls"))));
    visible.push_str(&f.feed(" done"));
    visible.push_str(&f.finish());
    assert_eq!(visible, "Here you go ! done");
  }

  #[test]
  fn filter_holds_back_a_marker_split_across_deltas() {
    let mut f = ToolCallLeakFilter::new();
    let a = f.feed("text <\u{FF5C}DS");
    let b = f.feed(&format!("ML\u{FF5C}tool_calls>ignore{}{}ok", close("invoke"), close("tool_calls")));
    let c = f.finish();
    assert_eq!(format!("{a}{b}{c}"), "text ok");
  }
}
