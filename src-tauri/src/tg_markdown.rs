// Converts the model's Markdown into the HTML subset Telegram accepts.
//
// Telegram has no headings, lists or tables, so those degrade to bold text,
// bullets and plain lines. Everything literal is escaped, and only tags
// Telegram actually supports are emitted — an unknown or unbalanced tag makes it
// reject the entire message with 400 rather than stripping it.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

// Telegram only needs &, < and > escaped in text.
fn escape_text(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  for ch in s.chars() {
    match ch {
      '&' => out.push_str("&amp;"),
      '<' => out.push_str("&lt;"),
      '>' => out.push_str("&gt;"),
      _ => out.push(ch),
    }
  }
  out
}

fn escape_attr(s: &str) -> String {
  escape_text(s).replace('"', "&quot;")
}

pub(crate) fn to_telegram_html(md: &str) -> String {
  let mut opts = Options::empty();
  opts.insert(Options::ENABLE_STRIKETHROUGH);

  let mut out = String::with_capacity(md.len() + 64);
  // Ordered lists carry their next number; unordered ones carry None.
  let mut lists: Vec<Option<u64>> = Vec::new();

  for ev in Parser::new_ext(md, opts) {
    match ev {
      Event::Start(tag) => match tag {
        Tag::Heading { .. } => out.push_str("<b>"),
        Tag::CodeBlock(_) => out.push_str("<pre>"),
        Tag::BlockQuote(..) => out.push_str("<blockquote>"),
        Tag::List(start) => {
          if !out.is_empty() && !out.ends_with('\n') { out.push('\n'); }
          lists.push(start);
        }
        Tag::Item => match lists.last_mut() {
          Some(Some(n)) => { out.push_str(&format!("{n}. ")); *n += 1; }
          _ => out.push_str("• "),
        },
        Tag::Emphasis => out.push_str("<i>"),
        Tag::Strong => out.push_str("<b>"),
        Tag::Strikethrough => out.push_str("<s>"),
        Tag::Link { dest_url, .. } => {
          out.push_str("<a href=\"");
          out.push_str(&escape_attr(&dest_url));
          out.push_str("\">");
        }
        _ => {}
      },
      Event::End(tag) => match tag {
        TagEnd::Paragraph => out.push('\n'),
        TagEnd::Heading(_) => out.push_str("</b>\n"),
        TagEnd::CodeBlock => out.push_str("</pre>\n"),
        TagEnd::BlockQuote(..) => out.push_str("</blockquote>\n"),
        TagEnd::List(..) => {
          lists.pop();
          if !out.ends_with("\n\n") { out.push('\n'); }
        }
        TagEnd::Item => out.push('\n'),
        TagEnd::Emphasis => out.push_str("</i>"),
        TagEnd::Strong => out.push_str("</b>"),
        TagEnd::Strikethrough => out.push_str("</s>"),
        TagEnd::Link => out.push_str("</a>"),
        _ => {}
      },
      Event::Text(t) => out.push_str(&escape_text(&t)),
      Event::Code(t) => {
        out.push_str("<code>");
        out.push_str(&escape_text(&t));
        out.push_str("</code>");
      }
      Event::SoftBreak => out.push('\n'),
      Event::HardBreak => out.push('\n'),
      Event::Rule => out.push_str("———\n"),
      // Raw HTML in model output is shown literally rather than forwarded, since
      // Telegram rejects any tag outside its subset.
      _ => {}
    }
  }

  // Collapse runs of blank lines: Telegram renders each one as dead space.
  let mut collapsed = String::with_capacity(out.len());
  let mut blanks = 0usize;
  for line in out.split('\n') {
    if line.trim().is_empty() {
      blanks += 1;
      if blanks > 1 { continue; }
    } else {
      blanks = 0;
    }
    collapsed.push_str(line);
    collapsed.push('\n');
  }
  collapsed.trim_end().to_string()
}
