//! The last 1 MiB of a terminal's output as plain text (ANSI stripped), for
//! `pty.read`. Offsets count chars since the terminal opened.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use regex::Regex;
use serde::Serialize;

const CAP_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PtyRead {
    pub text: String,
    /// Pass back as `since` to get only what's new.
    pub offset: u64,
    pub exited: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<u32>,
    /// Set when `waitFor` was given: whether it matched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched: Option<bool>,
    /// `since` was older than what's kept; `text` starts at the oldest kept char.
    pub truncated: bool,
}

#[derive(Default)]
struct Inner {
    text: String,
    /// Offset of `text`'s first char.
    base: u64,
    /// Offset after `text`'s last char.
    end: u64,
    exited: bool,
    exit_code: Option<u32>,
    strip: Strip,
}

#[derive(Default)]
pub struct Scrollback {
    inner: Mutex<Inner>,
    changed: Condvar,
}

impl Scrollback {
    pub fn push(&self, raw: &str) {
        let mut g = self.lock();
        let clean = g.strip.feed(raw);
        g.end += clean.chars().count() as u64;
        g.text.push_str(&clean);
        if g.text.len() > CAP_BYTES {
            let mut cut = g.text.len() - CAP_BYTES;
            while !g.text.is_char_boundary(cut) {
                cut += 1;
            }
            g.base += g.text[..cut].chars().count() as u64;
            g.text.drain(..cut);
        }
        drop(g);
        self.changed.notify_all();
    }

    pub fn exit(&self, code: Option<u32>) {
        let mut g = self.lock();
        g.exited = true;
        g.exit_code = code;
        drop(g);
        self.changed.notify_all();
    }

    /// Text after `since` (or everything kept). With `wait_for`, blocks until the
    /// new text matches, the terminal exits, or `timeout` runs out.
    pub fn read(&self, since: Option<u64>, wait_for: Option<&Regex>, timeout: Duration) -> PtyRead {
        let deadline = Instant::now() + timeout;
        let mut g = self.lock();
        loop {
            let from = since.unwrap_or(g.base);
            let truncated = from < g.base;
            let skip = from.clamp(g.base, g.end) - g.base;
            let start = g
                .text
                .char_indices()
                .nth(skip as usize)
                .map_or(g.text.len(), |(i, _)| i);
            let text = &g.text[start..];
            let matched = wait_for.map(|re| re.is_match(text));
            let now = Instant::now();
            if matched != Some(false) || g.exited || now >= deadline {
                return PtyRead {
                    text: text.to_string(),
                    offset: g.end,
                    exited: g.exited,
                    exit_code: g.exit_code,
                    matched,
                    truncated,
                };
            }
            g = self
                .changed
                .wait_timeout(g, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// `s` as plain text: ANSI/OSC sequences and control chars (but `\n`, `\t`) removed.
pub fn strip_ansi(s: &str) -> String {
    Strip::default().feed(s)
}

/// A streaming ANSI/control stripper: sequences split across chunks are handled.
#[derive(Default)]
enum Strip {
    #[default]
    Text,
    Esc,
    /// CSI: ESC [ … final byte in @..~
    Csi,
    /// OSC/DCS/etc.: ends at BEL or ESC \
    Str,
    StrEsc,
}

impl Strip {
    fn feed(&mut self, s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            *self = match (std::mem::take(self), c) {
                (Strip::Text, '\x1b') => Strip::Esc,
                (Strip::Text, '\n' | '\t') => {
                    out.push(c);
                    Strip::Text
                }
                // \r (line redraws), backspace, bells and other C0/C1 controls.
                (Strip::Text, c) if c.is_control() => Strip::Text,
                (Strip::Text, c) => {
                    out.push(c);
                    Strip::Text
                }
                (Strip::Esc, '[') => Strip::Csi,
                (Strip::Esc, ']' | 'P' | '_' | '^' | 'X') => Strip::Str,
                // Two-char escapes (ESC 7, ESC =, ESC (B …): the next char ends it,
                // except intermediates (space to /), which take one more.
                (Strip::Esc, ' '..='/') => Strip::Esc,
                (Strip::Esc, _) => Strip::Text,
                (Strip::Csi, '@'..='~') => Strip::Text,
                (Strip::Csi, _) => Strip::Csi,
                (Strip::Str, '\x07') => Strip::Text,
                (Strip::Str, '\x1b') => Strip::StrEsc,
                (Strip::Str, _) => Strip::Str,
                (Strip::StrEsc, '\\') => Strip::Text,
                (Strip::StrEsc, _) => Strip::Str,
            };
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_ansi_across_chunks() {
        let mut s = Strip::default();
        let mut out = s.feed("\x1b[1;3");
        out += &s.feed("2mred\x1b[0m ok\r\n");
        out += &s
            .feed("\x1b]0;title\x07PS> \x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\ ☃\x08\x1b(B\x1b=");
        assert_eq!(out, "red ok\nPS> link ☃");
    }

    #[test]
    fn offsets_since_and_truncation() {
        let sb = Scrollback::default();
        sb.push("héllo ");
        let r = sb.read(None, None, Duration::ZERO);
        assert_eq!(
            (r.text.as_str(), r.offset, r.truncated),
            ("héllo ", 6, false)
        );
        sb.push("\x1b[32mworld\x1b[0m");
        let r2 = sb.read(Some(r.offset), None, Duration::ZERO);
        assert_eq!((r2.text.as_str(), r2.offset), ("world", 11));

        // Past the cap, the oldest text goes and old offsets are flagged.
        let big = "x".repeat(CAP_BYTES);
        sb.push(&big);
        let r3 = sb.read(Some(0), None, Duration::ZERO);
        assert!(r3.truncated);
        assert_eq!(r3.text.len(), CAP_BYTES);
        assert_eq!(r3.offset, 11 + CAP_BYTES as u64);
        assert!(!sb.read(Some(r3.offset), None, Duration::ZERO).truncated);
    }

    #[test]
    fn wait_for_matches_new_text_or_times_out() {
        let sb = std::sync::Arc::new(Scrollback::default());
        sb.push("test result: old\n");
        let since = sb.read(None, None, Duration::ZERO).offset;
        let re = Regex::new(r"test result: \w+").unwrap();

        let writer = sb.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            writer.push("running…\n");
            std::thread::sleep(Duration::from_millis(50));
            writer.push("test result: ok\n");
        });
        let r = sb.read(Some(since), Some(&re), Duration::from_secs(5));
        t.join().unwrap();
        assert_eq!(r.matched, Some(true));
        assert!(
            r.text.ends_with("test result: ok\n"),
            "only new text is matched: {}",
            r.text
        );

        let start = Instant::now();
        let r = sb.read(Some(r.offset), Some(&re), Duration::from_millis(100));
        assert_eq!(r.matched, Some(false));
        assert!(start.elapsed() >= Duration::from_millis(100));

        let ender = sb.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            ender.exit(Some(2));
        });
        let r = sb.read(Some(r.offset), Some(&re), Duration::from_secs(5));
        assert_eq!(
            (r.exited, r.exit_code, r.matched),
            (true, Some(2), Some(false))
        );
    }
}
