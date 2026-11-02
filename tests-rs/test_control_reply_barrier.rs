// A control client's reply that reads pane state (capture-pane,
// display-message) describes exactly the %output sent before it, as in tmux,
// which queues a pane's output for its control clients and parses it in the
// same callback (window.c window_pane_read_callback) and runs a client's
// commands between two of them.
//
// psmux reads a pane on one thread, parses it on another after a coalescing
// wait of up to 8 ms, and drains the %output ring from the server loop, so a
// capture used to lag the %output already sent, or lead it, by up to a dozen
// lines on a busy pane. `hold_pane_output` is the barrier: it parses what the
// reader has read, drains the ring to the same byte, and holds the pane there
// until the client's batch is answered.
use super::*;
use crate::control::ReplyFence;
use std::sync::mpsc;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8};

/// A pane's PTY, fed from the test.
struct FedReader(mpsc::Receiver<Vec<u8>>);

impl std::io::Read for FedReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.0.recv() {
            Ok(bytes) => {
                let n = bytes.len().min(buf.len());
                buf[..n].copy_from_slice(&bytes[..n]);
                Ok(n)
            }
            Err(_) => Err(std::io::ErrorKind::BrokenPipe.into()),
        }
    }
}

struct TestPane {
    input: mpsc::Sender<Vec<u8>>,
    term: Arc<Mutex<vt100::Parser>>,
    ring: crate::control_ring::Ring,
}

impl TestPane {
    fn new(pane_id: usize) -> Self {
        let (input, rx) = mpsc::channel();
        let term = Arc::new(Mutex::new(vt100::Parser::new(24, 80, 1000)));
        let ring: crate::control_ring::Ring = Arc::new(Mutex::new(std::collections::VecDeque::new()));
        spawn_reader_thread(
            Box::new(FedReader(rx)),
            term.clone(),
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicU8::new(0)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicU32::new(0)),
            ring.clone(),
            pane_id,
            None,
        );
        TestPane { input, term, ring }
    }

    /// Write `bytes` as the pane's program and wait until the reader has read them.
    fn write(&self, bytes: &[u8]) {
        let before = self.ring.lock().unwrap().len();
        self.input.send(bytes.to_vec()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.ring.lock().unwrap().len() < before + bytes.len() {
            assert!(Instant::now() < deadline, "the reader never read the bytes");
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn screen(&self) -> String {
        self.term.lock().unwrap().screen().contents()
    }
}

fn lines(tag: &str, n: usize) -> Vec<u8> {
    (0..n).map(|i| format!("{tag}{i:04}\r\n")).collect::<String>().into_bytes()
}

#[test]
fn the_barrier_parses_everything_read_and_drains_the_ring_to_the_same_byte() {
    let pane = TestPane::new(970_001);
    // Over ECHO_CHUNK_MAX, so the parser thread would wait a coalescing tick.
    let first = lines("A", 200);
    pane.write(&first);
    let sent = hold_pane_output(&pane.ring, 1, Instant::now());
    assert_eq!(sent, first, "the reply follows every byte read so far as %output");
    assert!(pane.screen().contains("A0199"), "and the screen it describes has them all");
    release_pane_output(1);
}

#[test]
fn a_held_pane_stays_at_its_cut_for_the_rest_of_the_batch() {
    let pane = TestPane::new(970_002);
    pane.write(&lines("A", 50));
    // Taken as if 10 s from now, so a slow machine does not see the hold
    // run out (OUTPUT_HOLD_MAX) in the middle of the test.
    hold_pane_output(&pane.ring, 2, Instant::now() + Duration::from_secs(10));
    pane.write(b"LATER\r\n");
    thread::sleep(Duration::from_millis(30));
    assert!(pane_output_held(&pane.ring, Instant::now()));
    assert!(!pane.screen().contains("LATER"), "a held pane parses nothing new");
    // The next command of the same batch sees the same screen and is sent
    // no new output first.
    assert!(hold_pane_output(&pane.ring, 2, Instant::now()).is_empty());
    assert!(!pane.screen().contains("LATER"));
    // Another client's command joins the same cut: that cut's %output is
    // already queued ahead of every client's fence, so a new cut would put
    // output behind a fence whose reply already sees it.
    assert!(hold_pane_output(&pane.ring, 3, Instant::now()).is_empty());
    assert!(!pane.screen().contains("LATER"));
    // The pane stays held until every holder has released it.
    release_pane_output(3);
    assert!(pane_output_held(&pane.ring, Instant::now()));
    release_pane_output(2);
    assert!(!pane_output_held(&pane.ring, Instant::now()));
}

#[test]
fn a_released_pane_parses_again() {
    let pane = TestPane::new(970_003);
    pane.write(b"A\r\n");
    hold_pane_output(&pane.ring, 4, Instant::now());
    pane.write(b"AFTER\r\n");
    release_pane_output(4);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !pane.screen().contains("AFTER") {
        assert!(Instant::now() < deadline, "the parser never resumed");
        thread::sleep(Duration::from_millis(2));
    }
    // What arrived while held is still in the ring for the server loop.
    assert_eq!(crate::control_ring::drain(&pane.ring), b"AFTER\r\n".to_vec());
}

#[test]
fn a_hold_nobody_releases_runs_out() {
    let pane = TestPane::new(970_004);
    hold_pane_output(&pane.ring, 5, Instant::now());
    pane.write(b"STRAY\r\n");
    let deadline = Instant::now() + OUTPUT_HOLD_MAX + Duration::from_secs(5);
    while !pane.screen().contains("STRAY") {
        assert!(Instant::now() < deadline, "the hold never ran out");
        thread::sleep(Duration::from_millis(5));
    }
    assert!(!pane_output_held(&pane.ring, Instant::now()));
}

#[test]
fn a_reply_is_written_where_its_fence_was_queued() {
    let fence = Arc::new(ReplyFence::default());
    let wire = Arc::new(Mutex::new(Vec::<&str>::new()));
    let writer = {
        let (fence, wire) = (fence.clone(), wire.clone());
        thread::spawn(move || {
            wire.lock().unwrap().push("%output before");
            fence.pass(Duration::from_secs(10));
            wire.lock().unwrap().push("%output after");
        })
    };
    assert!(fence.wait_reached(Duration::from_secs(5)));
    thread::sleep(Duration::from_millis(20));
    wire.lock().unwrap().push("%begin .. %end");
    fence.release();
    writer.join().unwrap();
    assert_eq!(*wire.lock().unwrap(), vec!["%output before", "%begin .. %end", "%output after"]);
}

#[test]
fn a_fence_never_holds_up_a_reply_that_was_already_written_or_has_no_writer() {
    let written = ReplyFence::default();
    written.release();
    let t = Instant::now();
    written.pass(Duration::from_secs(10));
    assert!(t.elapsed() < Duration::from_secs(1));
    let orphan = ReplyFence::default();
    orphan.skip();
    assert!(orphan.wait_reached(Duration::from_secs(10)));
}
