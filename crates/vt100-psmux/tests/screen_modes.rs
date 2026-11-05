// The modes tmux reports as #{scroll_region_upper}, #{scroll_region_lower},
// #{origin_flag}, #{insert_flag}, #{wrap_flag}, #{keypad_cursor_flag} and
// #{keypad_flag} (format.c, read off wp->base), and where DECSTBM leaves the
// cursor.

use vt100_psmux as vt100;

fn screen_after(bytes: &[u8]) -> vt100::Screen {
    let mut parser = vt100::Parser::new(24, 80, 0);
    parser.process(bytes);
    parser.screen().clone()
}

#[test]
fn a_fresh_screen_has_the_default_modes() {
    let s = screen_after(b"");
    assert_eq!(s.scroll_region(), (0, 23));
    assert!(!s.origin_mode());
    assert!(!s.insert_mode());
    assert!(s.autowrap());
    assert!(!s.application_cursor());
    assert!(!s.application_keypad());
}

#[test]
fn decstbm_sets_the_scroll_region() {
    assert_eq!(screen_after(b"\x1b[5;20r").scroll_region(), (4, 19));
    assert_eq!(screen_after(b"\x1b[5;20r\x1b[r").scroll_region(), (0, 23));
}

// xterm and VT510: DECSTBM moves the cursor to the home position, which is the
// top of the scroll region only while origin mode is on.
#[test]
fn decstbm_homes_the_cursor() {
    assert_eq!(screen_after(b"\x1b[10;10H\x1b[5;20r").cursor_position(), (0, 0));
    let s = screen_after(b"\x1b[?6h\x1b[10;10H\x1b[5;20r");
    assert!(s.origin_mode());
    assert_eq!(s.cursor_position(), (4, 0));
}

#[test]
fn decom_is_reported() {
    let s = screen_after(b"\x1b[5;20r\x1b[?6h");
    assert!(s.origin_mode());
    assert_eq!(s.cursor_position(), (4, 0));
    assert!(!screen_after(b"\x1b[?6h\x1b[?6l").origin_mode());
}

#[test]
fn irm_is_reported() {
    assert!(screen_after(b"\x1b[4h").insert_mode());
    assert!(!screen_after(b"\x1b[4h\x1b[4l").insert_mode());
    // SM without a question mark is not DECSET: CSI 4 h is not CSI ? 4 h.
    assert!(!screen_after(b"\x1b[?4h").insert_mode());
}

#[test]
fn decawm_is_reported() {
    assert!(!screen_after(b"\x1b[?7l").autowrap());
    assert!(screen_after(b"\x1b[?7l\x1b[?7h").autowrap());
    assert!(screen_after(b"\x1b[?7l\x1bc").autowrap(), "RIS turns autowrap back on");
}

#[test]
fn keypad_modes_are_reported() {
    assert!(screen_after(b"\x1b[?1h").application_cursor());
    assert!(screen_after(b"\x1b=").application_keypad());
    assert!(!screen_after(b"\x1b=\x1b>").application_keypad());
}
