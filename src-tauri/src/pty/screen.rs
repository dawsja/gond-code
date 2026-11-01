//! A terminal's screen as the terminal host keeps it: every byte the program prints goes through
//! a terminal parser (`vt100`), so a pane that attaches later gets the exact screen, a long
//! history, and the modes the program switched on, not a tail of raw bytes that may start in
//! the middle of a full-screen app or after the program turned bracketed paste on.
//!
//! `render` writes it all back out as escape codes for xterm.js: the history (oldest first),
//! the visible screen, the input modes (keypad, cursor keys, bracketed paste, mouse mode and
//! encoding, cursor style), focus reporting (which `vt100` leaves to us), and the cursor.

/// History kept per terminal, in lines. A line is 32 bytes a column, so 4000 lines of 120
/// columns is about 15 MB once full; a fresh terminal uses next to nothing.
pub const HISTORY: usize = 4000;

/// The modes `vt100` doesn't keep but a pane must get back: focus reporting (`?1004`).
#[derive(Default)]
pub struct Extra {
    focus_reporting: bool,
}

impl vt100::Callbacks for Extra {
    fn unhandled_csi(
        &mut self,
        _: &mut vt100::Screen,
        i1: Option<u8>,
        _i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        if i1 != Some(b'?') || !matches!(c, 'h' | 'l') {
            return;
        }
        for p in params {
            if p.first() == Some(&1004) {
                self.focus_reporting = c == 'h';
            }
        }
    }
}

/// One terminal's parsed screen, and how many bytes of output it has seen.
pub struct ScreenState {
    parser: vt100::Parser<Extra>,
    /// Output length so far, in bytes (`pty:data`'s `end`).
    pub end: u64,
}

fn nz(n: u16) -> core::num::NonZeroU16 {
    core::num::NonZeroU16::new(n.max(1)).unwrap_or(core::num::NonZeroU16::MIN)
}

impl ScreenState {
    pub fn new(cols: u16, rows: u16) -> Self {
        ScreenState {
            parser: vt100::Parser::new_with_callbacks(
                nz(rows),
                nz(cols),
                HISTORY,
                Extra::default(),
            ),
            end: 0,
        }
    }

    /// Takes in output the program printed.
    pub fn push(&mut self, chunk: &str) {
        self.parser.process(chunk.as_bytes());
        self.end += chunk.len() as u64;
    }

    /// Follows the terminal's size.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.parser.screen_mut().set_size(nz(rows), nz(cols));
    }

    /// Escape codes that draw this terminal, history and all, into a fresh xterm.js.
    pub fn render(&mut self) -> String {
        let mut out = String::from("\x1b[0m\x1b[H\x1b[2J");
        let (rows, cols) = {
            let (r, c) = self.parser.screen().size();
            (r.get(), c.get())
        };
        let alternate = self.parser.screen().alternate_screen();
        if alternate {
            // A full-screen app: its own screen, which it redraws whole anyway.
            out.push_str("\x1b[?1049h");
        } else {
            self.write_history(&mut out, rows, cols);
            // Push the last history lines up out of the screen, so drawing it doesn't erase them.
            for _ in 0..rows {
                out.push_str("\r\n");
            }
        }
        let screen = self.parser.screen();
        out.push_str(&screen.contents_formatted());
        out.push_str(&screen.input_mode_formatted());
        if self.parser.callbacks().focus_reporting {
            out.push_str("\x1b[?1004h");
        }
        out.push_str(&screen.cursor_state_formatted());
        out
    }

    /// The history rows, oldest first, each as its own line.
    fn write_history(&mut self, out: &mut String, rows: u16, cols: u16) {
        let screen = self.parser.screen_mut();
        let total = {
            screen.set_scrollback(usize::MAX);
            screen.scrollback()
        };
        // At scrollback offset `k` the top visible row is history row `total - k`: a page at a
        // time from the oldest.
        let mut k = total;
        while k > 0 {
            screen.set_scrollback(k);
            let take = (rows as usize).min(k);
            for row in screen.rows_formatted(0, cols).take(take) {
                out.push_str(&row);
                out.push_str("\x1b[0m\r\n");
            }
            k -= take;
        }
        screen.set_scrollback(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All the text a parser holds: its history then its screen.
    fn all_text(p: &mut vt100::Parser<Extra>) -> Vec<String> {
        let (rows, cols) = {
            let (r, c) = p.screen().size();
            (r.get(), c.get())
        };
        let mut lines = vec![];
        let s = p.screen_mut();
        s.set_scrollback(usize::MAX);
        let mut k = s.scrollback();
        while k > 0 {
            s.set_scrollback(k);
            let take = (rows as usize).min(k);
            lines.extend(s.rows(0, cols).take(take));
            k -= take;
        }
        s.set_scrollback(0);
        lines.extend(s.rows(0, cols));
        lines
    }

    /// Draws `state` into a fresh parser of the same size, as a pane would.
    fn redrawn(state: &mut ScreenState, cols: u16, rows: u16) -> vt100::Parser<Extra> {
        let mut fresh =
            vt100::Parser::new_with_callbacks(nz(rows), nz(cols), 100_000, Extra::default());
        fresh.process(state.render().as_bytes());
        fresh
    }

    #[test]
    fn a_redrawn_pane_has_the_same_history_and_screen() {
        let mut s = ScreenState::new(40, 8);
        for i in 0..60 {
            s.push(&format!("line \x1b[31m{i}\x1b[0m of history\r\n"));
        }
        s.push("$ prompt");
        let mut fresh = redrawn(&mut s, 40, 8);
        let want: Vec<String> = all_text(&mut s.parser)
            .into_iter()
            .filter(|l| !l.is_empty())
            .collect();
        let got: Vec<String> = all_text(&mut fresh)
            .into_iter()
            .filter(|l| !l.is_empty())
            .collect();
        assert_eq!(got, want);
        assert_eq!(
            fresh.screen().cursor_position(),
            s.parser.screen().cursor_position()
        );
        // Colour comes back too.
        assert_eq!(
            fresh.screen().cell(0, 5).map(|c| c.fgcolor()),
            s.parser.screen().cell(0, 5).map(|c| c.fgcolor()),
        );
    }

    #[test]
    fn modes_switched_on_long_ago_come_back() {
        let mut s = ScreenState::new(80, 24);
        // Turned on first, then buried under a lot of output.
        s.push("\x1b[?2004h\x1b[?1004h\x1b[?1h\x1b[?1000h\x1b[?1006h");
        for i in 0..20_000 {
            s.push(&format!("busy output {i}\r\n"));
        }
        let fresh = redrawn(&mut s, 80, 24);
        let screen = fresh.screen();
        assert!(screen.bracketed_paste());
        assert!(screen.application_cursor());
        assert_eq!(
            screen.mouse_protocol_mode(),
            vt100::MouseProtocolMode::PressRelease
        );
        assert_eq!(
            screen.mouse_protocol_encoding(),
            vt100::MouseProtocolEncoding::Sgr
        );
        assert!(fresh.callbacks().focus_reporting);
        // And off again when the program turned them off.
        s.push("\x1b[?2004l\x1b[?1004l");
        let fresh = redrawn(&mut s, 80, 24);
        assert!(!fresh.screen().bracketed_paste());
        assert!(!fresh.callbacks().focus_reporting);
    }

    #[test]
    fn a_full_screen_app_comes_back_on_its_own_screen() {
        let mut s = ScreenState::new(30, 5);
        s.push("shell before\r\n\x1b[?1049h\x1b[H\x1b[2Jthe app's screen");
        let fresh = redrawn(&mut s, 30, 5);
        assert!(fresh.screen().alternate_screen());
        assert!(fresh.screen().contents().contains("the app's screen"));
    }

    #[test]
    fn history_is_capped() {
        let mut s = ScreenState::new(20, 4);
        for i in 0..(HISTORY + 500) {
            s.push(&format!("{i}\r\n"));
        }
        let lines = all_text(&mut s.parser);
        assert!(lines.len() <= HISTORY + 4);
        assert!(lines.iter().any(|l| l == &format!("{}", HISTORY + 499)));
    }
}
