// #{alternate_on}, #{scroll_region_upper}, #{scroll_region_lower},
// #{origin_flag}, #{insert_flag}, #{wrap_flag}, #{keypad_cursor_flag} and
// #{keypad_flag} describe the TARGET pane's own screen, as tmux's format.c
// reads them off ft->wp->base.  psmux answered alternate_on and
// scroll_region_lower from the active pane, and the rest were constants
// (upper 0, lower the active pane's last row, wrap 1, everything else 0), so
// `list-panes -F` and `display-message -t %N` misreported every pane but the
// active one and every pane whose application had changed a mode.

use crate::types::{AppState, LayoutKind, Node, Pane, Window};
use std::net::{TcpListener, TcpStream};

const LEFT: usize = 401;
const RIGHT: usize = 402;
const COLS: u16 = 40;
const ROWS: u16 = 24;

const VARS: &str = "#{pane_id}:alt=#{alternate_on} sr=#{scroll_region_upper},#{scroll_region_lower} \
                    origin=#{origin_flag} insert=#{insert_flag} wrap=#{wrap_flag} \
                    kcur=#{keypad_cursor_flag} kpad=#{keypad_flag}";

fn tcp_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let addr = listener.local_addr().expect("listener addr");
    let accept_thr = std::thread::spawn(move || listener.accept().expect("accept").0);
    let client = TcpStream::connect(addr).expect("connect");
    let server = accept_thr.join().expect("join accept thread");
    (client, server)
}

fn make_pane(id: usize) -> (Pane, TcpStream, TcpStream) {
    let (reader, reader_peer) = tcp_pair();
    let (writer, peer) = tcp_pair();
    let pane = crate::proxy_pane::create_proxy_pane(
        reader,
        writer,
        "127.0.0.1:1".to_string(),
        "test-key".to_string(),
        "test-session".to_string(),
        id as u64,
        None,
        format!("pane-{id}"),
        ROWS,
        COLS,
        id,
        None,
    )
    .expect("create proxy pane");
    (pane, peer, reader_peer)
}

fn feed(pane: &Pane, bytes: &[u8]) {
    pane.term.lock().expect("term lock").process(bytes);
}

/// One window split in two; the right pane is the active one.
struct Harness {
    app: AppState,
    _streams: Vec<TcpStream>,
}

impl Harness {
    fn new() -> Harness {
        let (left, p1, r1) = make_pane(LEFT);
        let (right, p2, r2) = make_pane(RIGHT);
        let mut app = AppState::new("screen-modes".to_string());
        app.window_base_index = 0;
        app.windows.push(Window {
            root: Node::Split {
                kind: LayoutKind::Horizontal,
                sizes: vec![50, 50],
                children: vec![Node::Leaf(left), Node::Leaf(right)],
            },
            active_path: vec![1],
            name: "w0".to_string(),
            id: 0,
            area: app.client_area,
            window_size: None,
            window_options: Default::default(),
            activity_flag: false,
            bell_flag: false,
            silence_flag: false,
            last_output_time: std::time::Instant::now(),
            last_seen_version: 0,
            manual_rename: false,
            layout_index: 0,
            pane_mru: vec![RIGHT, LEFT],
            zoom_saved: None,
            linked_from: None,
            floating: Vec::new(),
            floating_focus: None,
        });
        Harness { app, _streams: vec![p1, r1, p2, r2] }
    }

    fn emit(&self, child: usize, bytes: &[u8]) {
        let Node::Split { children, .. } = &self.app.windows[0].root else {
            panic!("harness window is not split");
        };
        let Node::Leaf(pane) = &children[child] else {
            panic!("harness child is not a leaf");
        };
        feed(pane, bytes);
    }

    fn panes(&self) -> Vec<String> {
        super::format_list_panes(&self.app, VARS, 0).lines().map(str::to_string).collect()
    }
}

fn defaults(id: usize) -> String {
    format!("%{id}:alt=0 sr=0,{} origin=0 insert=0 wrap=1 kcur=0 kpad=0", ROWS - 1)
}

#[test]
fn panes_that_changed_nothing_report_the_defaults() {
    let h = Harness::new();
    assert_eq!(h.panes(), vec![defaults(LEFT), defaults(RIGHT)]);
}

#[test]
fn an_inactive_pane_reports_its_own_modes() {
    let h = Harness::new();
    h.emit(0, b"\x1b[?1049h\x1b[5;20r\x1b[?6h\x1b[4h\x1b[?7l\x1b[?1h\x1b=");
    assert_eq!(
        h.panes(),
        vec![
            format!("%{LEFT}:alt=1 sr=4,19 origin=1 insert=1 wrap=0 kcur=1 kpad=1"),
            defaults(RIGHT),
        ]
    );
}

#[test]
fn the_active_pane_does_not_lend_its_modes_to_the_others() {
    let h = Harness::new();
    h.emit(1, b"\x1b[?1049h\x1b[3;10r\x1b[?1h");
    assert_eq!(
        h.panes(),
        vec![
            defaults(LEFT),
            format!("%{RIGHT}:alt=1 sr=2,9 origin=0 insert=0 wrap=1 kcur=1 kpad=0"),
        ]
    );
}

#[test]
fn modes_turned_off_again_read_as_off() {
    let h = Harness::new();
    h.emit(0, b"\x1b[?1049h\x1b[5;20r\x1b[?6h\x1b[4h\x1b[?7l\x1b[?1h\x1b=");
    h.emit(0, b"\x1b[?1049l\x1b[r\x1b[?6l\x1b[4l\x1b[?7h\x1b[?1l\x1b>");
    assert_eq!(h.panes(), vec![defaults(LEFT), defaults(RIGHT)]);
}
