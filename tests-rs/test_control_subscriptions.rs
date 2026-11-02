// `refresh-client -B name:what:format` watches the object `what` names, as
// tmux's control.c does: `%*` expands the format once for every pane of the
// session and reports each pane's own value with its own ids, `%N` one pane,
// `@*` and `@N` windows, anything else the session. psmux stored `what` and
// ignored it, expanding every subscription once, on the active pane, and
// naming the active window and pane in the notification, so a change in any
// other pane was never reported. Registered from src/control.rs.

use super::*;
use crate::types::Pane;
use std::net::{TcpListener, TcpStream};

const A: usize = 11;
const B: usize = 12;
const C: usize = 13;
const W0: usize = 3;
const W1: usize = 4;

fn tcp_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let addr = listener.local_addr().expect("listener addr");
    let accept_thr = std::thread::spawn(move || listener.accept().expect("accept").0);
    let client = TcpStream::connect(addr).expect("connect");
    let server = accept_thr.join().expect("join accept thread");
    (client, server)
}

fn make_pane(id: usize, streams: &mut Vec<TcpStream>) -> Pane {
    let (reader, reader_peer) = tcp_pair();
    let (writer, peer) = tcp_pair();
    streams.push(reader_peer);
    streams.push(peer);
    let mut pane = crate::proxy_pane::create_proxy_pane(
        reader,
        writer,
        "127.0.0.1:1".to_string(),
        "test-key".to_string(),
        "test-session".to_string(),
        id as u64,
        None,
        format!("pane-{id}"),
        10,
        40,
        id,
        None,
    )
    .expect("create proxy pane");
    pane.pane_options.insert("@fg".to_string(), format!("cmd-{id}"));
    pane
}

fn window(id: usize, name: &str, root: Node, active_path: Vec<usize>, area: ratatui::layout::Rect) -> Window {
    Window {
        root,
        active_path,
        name: name.to_string(),
        id,
        area,
        window_size: None,
        window_options: Default::default(),
        activity_flag: false,
        bell_flag: false,
        silence_flag: false,
        last_output_time: std::time::Instant::now(),
        last_seen_version: 0,
        manual_rename: false,
        layout_index: 0,
        pane_mru: vec![],
        zoom_saved: None,
        linked_from: None,
        floating: Vec::new(),
        floating_focus: None,
    }
}

/// Window @3 (current) split into %11 and %12, %12 active; window @4 holds %13.
fn session(streams: &mut Vec<TcpStream>) -> AppState {
    let mut app = AppState::new("subs".to_string());
    app.window_base_index = 0;
    let split = Node::Split {
        kind: LayoutKind::Horizontal,
        sizes: vec![50, 50],
        children: vec![Node::Leaf(make_pane(A, streams)), Node::Leaf(make_pane(B, streams))],
    };
    let area = app.client_area;
    app.windows.push(window(W0, "one", split, vec![1], area));
    app.windows.push(window(W1, "two", Node::Leaf(make_pane(C, streams)), vec![], area));
    app.active_idx = 0;
    app
}

fn lines(app: &AppState, what: &str, format: &str) -> Vec<String> {
    evaluate_subscription(app, what, format)
        .into_iter()
        .map(|(object, value)| {
            format_notification(&ControlNotification::SubscriptionChanged {
                name: "s".to_string(),
                session_id: 0,
                object,
                value,
            })
        })
        .collect()
}

#[test]
fn what_is_read_as_tmux_reads_it() {
    assert_eq!(parse_subscription_target("%*"), SubscriptionTarget::AllPanes);
    assert_eq!(parse_subscription_target("%7"), SubscriptionTarget::Pane(7));
    assert_eq!(parse_subscription_target("@*"), SubscriptionTarget::AllWindows);
    assert_eq!(parse_subscription_target("@2"), SubscriptionTarget::Window(2));
    assert_eq!(parse_subscription_target("$0"), SubscriptionTarget::Session);
    assert_eq!(parse_subscription_target(""), SubscriptionTarget::Session);
    assert_eq!(parse_subscription_target("%x"), SubscriptionTarget::Session);
}

#[test]
fn every_pane_is_expanded_for_itself() {
    let mut streams = Vec::new();
    let app = session(&mut streams);
    assert_eq!(
        lines(&app, "%*", "#{pane_id} #{pane_active} #{@fg}"),
        vec![
            format!("%subscription-changed s $0 @{W0} 0 %{A} : %{A} 0 cmd-{A}"),
            format!("%subscription-changed s $0 @{W0} 0 %{B} : %{B} 1 cmd-{B}"),
            format!("%subscription-changed s $0 @{W1} 1 %{C} : %{C} 1 cmd-{C}"),
        ]
    );
}

#[test]
fn one_pane_in_a_window_that_is_not_current() {
    let mut streams = Vec::new();
    let app = session(&mut streams);
    assert_eq!(
        lines(&app, &format!("%{C}"), "#{pane_id} #{window_id}"),
        vec![format!("%subscription-changed s $0 @{W1} 1 %{C} : %{C} @{W1}")]
    );
    assert!(lines(&app, "%99", "#{pane_id}").is_empty(), "a pane that does not exist reports nothing");
}

#[test]
fn windows_name_no_pane() {
    let mut streams = Vec::new();
    let app = session(&mut streams);
    assert_eq!(
        lines(&app, "@*", "#{window_name} #{pane_id}"),
        vec![
            format!("%subscription-changed s $0 @{W0} 0 - : one %{B}"),
            format!("%subscription-changed s $0 @{W1} 1 - : two %{C}"),
        ]
    );
    assert_eq!(
        lines(&app, &format!("@{W1}"), "#{window_name}"),
        vec![format!("%subscription-changed s $0 @{W1} 1 - : two")]
    );
}

#[test]
fn the_session_names_no_window_or_pane() {
    let mut streams = Vec::new();
    let app = session(&mut streams);
    assert_eq!(
        lines(&app, "$", "#{session_name} #{pane_id}"),
        vec![format!("%subscription-changed s $0 - - - : subs %{B}")]
    );
}
