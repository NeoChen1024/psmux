// break-pane, join-pane and move-pane change the layout of the window the
// pane LEAVES as well as the one it lands in, and only the current window was
// resized afterwards. After `break-pane` (no -d) the pane left beside the
// broken one was still 29 columns wide in a 59 column cell, PTY included,
// until the next resize.
//
// tmux lays out both windows (cmd-break-pane.c layout_close_pane,
// cmd-join-pane.c server_redraw_window on the source and the target). These
// tests drive the real operations on panes backed by stub PTYs, as
// tests-rs/test_issue689_break_swap_pane.rs does. Registered from
// src/window_ops.rs.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::types::{AppState, LayoutKind, Node};
use crate::window_ops::BreakPaneRequest;
use ratatui::layout::Rect;

fn make_pane(id: usize, rows: u16, cols: u16) -> crate::types::Pane {
    let (master, writer) = crate::util::stub_pane_pty(portable_pty::PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
    let child = crate::util::StubChild::exited();
    let term = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 0)));
    let epoch = Instant::now() - Duration::from_secs(2);
    crate::types::Pane {
        master,
        writer,
        child,
        term,
        last_rows: rows,
        last_cols: cols,
        id,
        title: format!("pane{id}"),
        title_locked: false,
        child_pid: None,
        data_version: Arc::new(AtomicU64::new(0)),
        last_title_check: epoch,
        last_infer_title: epoch,
        dead: false,
        dead_info: None,
        last_text_input: None,
        last_special_key: None,
        vt_bridge_cache: None,
        vti_mode_cache: None,
        mouse_input_cache: None, win32_input_latched: false, input_off: false,
        scroll_fg_cache: None, mouse_proto_owner: None, wheel_auth: None,
        cursor_shape: Arc::new(AtomicU8::new(0)),
        bell_pending: Arc::new(AtomicBool::new(false)),
        cpr_pending: Arc::new(AtomicBool::new(false)),
        color_query_pending: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        copy_state: None, live_term: None,
        pane_style: None, pane_options: Default::default(),
        squelch_until: None,
        output_ring: Arc::new(Mutex::new(std::collections::VecDeque::new())),
        spawned_at: None,
        start_command: String::new(),
        cwd_hint: None,
    }
}

/// Window w holds the pane ids given, side by side in a 160x40 window, and
/// every pane already has the size its cell gives it.
fn app_with_windows(windows: &[&[usize]]) -> AppState {
    let mut app = AppState::new("layoutchange".to_string());
    app.window_base_index = 0;
    app.pane_base_index = 0;
    let area = Rect::new(0, 0, 160, 40);
    app.last_window_area = area;
    app.client_area = area;
    for (w, ids) in windows.iter().enumerate() {
        let root = if ids.len() == 1 {
            Node::Leaf(make_pane(ids[0], 40, 160))
        } else {
            let n = ids.len();
            Node::Split {
                kind: LayoutKind::Horizontal,
                sizes: vec![(100 / n) as u16; n],
                children: ids.iter().map(|&id| Node::Leaf(make_pane(id, 40, 160 / n as u16))).collect(),
            }
        };
        app.windows.push(crate::types::Window {
            active_path: if ids.len() == 1 { vec![] } else { vec![0] },
            root,
            name: format!("win{w}"),
            id: w,
            area,
            window_size: None,
            window_options: Default::default(),
            activity_flag: false,
            bell_flag: false,
            silence_flag: false,
            last_output_time: Instant::now(),
            last_seen_version: 0,
            manual_rename: false,
            layout_index: 0,
            pane_mru: ids.to_vec(),
            zoom_saved: None,
            linked_from: None,
            floating: Vec::new(),
            floating_focus: None,
        });
        app.window_indices.push(w);
        crate::tree::resize_window_panes(&mut app, w, area);
    }
    app.next_win_id = windows.len();
    app.active_idx = 0;
    app
}

/// Every pane of window `w` has the size of the cell its layout gives it,
/// PTY and screen alike. Returns how many panes there are.
fn assert_laid_out(app: &AppState, w: usize, what: &str) -> usize {
    let win = &app.windows[w];
    let mut rects = Vec::new();
    crate::tree::compute_rects(&win.root, win.area, &mut rects);
    for (path, rect) in &rects {
        let pane = crate::tree::active_pane(&win.root, path).expect("a leaf");
        let (rows, cols) = crate::tree::pane_inner_size(*rect, 0);
        assert_eq!((pane.last_rows, pane.last_cols), (rows, cols), "{what}: pane %{} in a {}x{} cell", pane.id, rect.width, rect.height);
        let screen = pane.term.lock().map(|t| t.screen().size()).unwrap_or((0, 0));
        assert_eq!(screen, (rows, cols), "{what}: screen of pane %{}", pane.id);
    }
    rects.len()
}

#[test]
fn break_pane_resizes_the_panes_left_in_the_source_window() {
    let mut app = app_with_windows(&[&[1, 2, 3]]);
    app.windows[0].active_path = vec![2];
    crate::window_ops::break_pane(&mut app, &BreakPaneRequest::default()).expect("breaks");

    assert_eq!(app.active_idx, 1, "without -d the new window is current");
    assert_eq!(assert_laid_out(&app, 0, "source window"), 2);
}

#[test]
fn join_pane_resizes_the_source_and_the_target_window() {
    for detach in [false, true] {
        let mut app = app_with_windows(&[&[1, 2, 3], &[4]]);
        app.windows[0].active_path = vec![2];
        assert!(crate::commands::join_pane_local(&mut app, Some(0), Some(2), Some(1), None, true, detach, false, None));

        assert_eq!(app.active_idx, if detach { 0 } else { 1 });
        assert_eq!(assert_laid_out(&app, 0, &format!("source window, -d {detach}")), 2);
        assert_eq!(assert_laid_out(&app, 1, &format!("target window, -d {detach}")), 2);
    }
}

#[test]
fn move_pane_with_detach_resizes_a_window_that_is_not_current() {
    let mut app = app_with_windows(&[&[1], &[2, 3, 4]]);
    crate::commands::move_pane_in_window(&mut app, 1, Some(0), Some(2), false, true, false, None).expect("moves");

    assert_eq!(app.active_idx, 0, "-d leaves the current window alone");
    assert_eq!(assert_laid_out(&app, 1, "moved within @1"), 3);
}

