// capture-pane -S/-E with negative lines while the pane is on the alternate
// screen.
//
// tmux keeps a pane's history under its alternate screen
// (screen.c screen_alternate_on clears only the visible lines and stops adding
// history), so `capture-pane -S -N` reads the same history lines whichever
// screen is up, and `#{history_size}` counts them. psmux's vt100 keeps the
// alternate screen in a separate grid with no scrollback, and the capture
// walked that grid: every negative line was clamped to the alternate screen's
// first row, so `-S -5 -E -1` returned that row once and `-S -` returned only
// the alternate screen, while `#{history_size}` still counted the main grid's
// history.
//
// Registered from src/copy_mode.rs; the pane scaffolding (stub PTY, no psmux
// server) is shared with test_capture_pane_fidelity.rs.

use super::*;
use super::tests_capture_pane_fidelity::{app_showing, make_pane, make_window};
use crate::types::Node;
use std::sync::{Arc, Mutex};

const ROWS: u16 = 6;

/// A 6x40 pane that printed L00..L19 and so holds L00..L14 as history,
/// then switched to the alternate screen and drew two rows on it.
fn app_on_alternate_screen() -> AppState {
    let pane = {
        let mut p = make_pane(0, ROWS, 40);
        p.term = Arc::new(Mutex::new(vt100::Parser::new(ROWS, 40, 100)));
        p
    };
    let mut bytes = String::new();
    for i in 0..20 {
        bytes.push_str(&format!("L{i:02}\r\n"));
    }
    pane.term.lock().unwrap().process(bytes.as_bytes());
    pane.term.lock().unwrap().process(b"\x1b[?1049h\x1b[HALT-1\r\nALT-2");
    let mut app = app_showing(b"");
    let mut win = make_window(0);
    win.root = Node::Leaf(pane);
    app.windows[0] = win;
    app
}

fn history_size(app: &mut AppState) -> i32 {
    let win = &mut app.windows[0];
    let p = crate::tree::active_pane_mut(&mut win.root, &win.active_path).unwrap();
    let parser = p.term.lock().unwrap();
    assert!(parser.screen().alternate_screen());
    parser.screen().scrollback_filled() as i32
}

fn lines(range: &[&str]) -> String {
    range.iter().map(|l| format!("{l}\n")).collect()
}

#[test]
fn negative_lines_read_the_history_under_the_alternate_screen() {
    let mut app = app_on_alternate_screen();
    assert_eq!(history_size(&mut app), 15);
    let last = capture_active_pane_range(&mut app, Some(-5), Some(-1), None, false).unwrap().unwrap();
    assert_eq!(last, lines(&["L10", "L11", "L12", "L13", "L14"]));
    let oldest = capture_active_pane_range(&mut app, Some(-15), Some(-13), None, false).unwrap().unwrap();
    assert_eq!(oldest, lines(&["L00", "L01", "L02"]));
}

#[test]
fn a_range_across_the_top_runs_from_the_history_into_the_alternate_screen() {
    let mut app = app_on_alternate_screen();
    let out = capture_active_pane_range(&mut app, Some(-2), Some(1), None, false).unwrap().unwrap();
    assert_eq!(out, lines(&["L13", "L14", "ALT-1", "ALT-2"]));
    // `-S -` is the whole history and then the screen in use.
    let all = capture_active_pane_range(&mut app, Some(i32::MIN), None, None, false).unwrap().unwrap();
    let mut want: Vec<String> = (0..15).map(|i| format!("L{i:02}")).collect();
    want.extend(["ALT-1", "ALT-2", "", "", "", ""].map(String::from));
    assert_eq!(all, want.iter().map(|l| format!("{l}\n")).collect::<String>());
}

#[test]
fn styled_capture_reads_the_same_history() {
    let mut app = app_on_alternate_screen();
    let out = capture_active_pane_styled(&mut app, Some(-3), Some(0), None, false).unwrap().unwrap();
    let text = out.replace("\x1b[0m", "");
    assert_eq!(text, lines(&["L12", "L13", "L14", "ALT-1"]), "got {out:?}");
}

#[test]
fn the_capture_leaves_the_alternate_screen_in_view() {
    let mut app = app_on_alternate_screen();
    capture_active_pane_range(&mut app, Some(-5), Some(-1), None, false).unwrap();
    let screen = capture_active_pane_text(&mut app, None, false).unwrap().unwrap();
    assert_eq!(screen, lines(&["ALT-1", "ALT-2"]));
    let win = &mut app.windows[0];
    let p = crate::tree::active_pane_mut(&mut win.root, &win.active_path).unwrap();
    let parser = p.term.lock().unwrap();
    assert!(parser.screen().alternate_screen());
    assert_eq!(parser.screen().scrollback(), 0);
}
