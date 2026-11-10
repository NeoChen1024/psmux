// The main screen reflows on resize the way conhost reflows the pseudo
// console's buffer behind every pane. ConPTY does not repaint after a resize,
// so if the two layouts differ, later output lands on the wrong rows.

fn rows(parser: &vt100_psmux::Parser) -> Vec<String> {
    let (_, cols) = parser.screen().size();
    parser.screen().rows(0, cols).collect()
}

const LONG: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123";

fn prompt_screen() -> vt100_psmux::Parser {
    let mut parser = vt100_psmux::Parser::new(6, 20, 100);
    parser.process(format!("first\r\n\r\n{LONG}\r\nPS C:\\> ").as_bytes());
    assert_eq!(parser.screen().cursor_position(), (4, 8));
    parser
}

#[test]
fn width_shrink_rewraps_and_keeps_the_prompt_with_the_cursor() {
    let mut parser = prompt_screen();
    parser.screen_mut().set_size(5, 12);

    assert_eq!(
        rows(&parser),
        ["", "ABCDEFGHIJKL", "MNOPQRSTUVWX", "YZ0123", "PS C:\\>"]
    );
    assert!(parser.screen().row_wrapped(1));
    assert!(parser.screen().row_wrapped(2));
    assert!(!parser.screen().row_wrapped(3));
    // The trailing space of the prompt is kept as the cursor's offset.
    assert_eq!(parser.screen().cursor_position(), (4, 8));
    // Only the row needed to keep the cursor on screen went to history.
    assert_eq!(parser.screen().scrollback_filled(), 1);

    parser.process(b"ls");
    assert_eq!(rows(&parser)[4], "PS C:\\> ls");
    assert_eq!(parser.screen().cursor_position(), (4, 10));
}

#[test]
fn width_grow_joins_wrapped_rows_again() {
    let mut parser = prompt_screen();
    parser.screen_mut().set_size(5, 12);
    parser.process(b"ls");
    parser.screen_mut().set_size(6, 20);

    assert_eq!(
        rows(&parser),
        ["", "ABCDEFGHIJKLMNOPQRST", "UVWXYZ0123", "PS C:\\> ls", "", ""]
    );
    assert!(parser.screen().row_wrapped(1));
    assert_eq!(parser.screen().cursor_position(), (3, 10));
    // History is not pulled back onto the screen when it grows.
    assert_eq!(parser.screen().scrollback_filled(), 1);
    assert!(parser.screen().contents().contains(LONG));

    parser.process(b"\r\nPS C:\\> echo hi");
    assert_eq!(rows(&parser)[4], "PS C:\\> echo hi");
}

#[test]
fn height_shrink_with_the_cursor_at_the_bottom_scrolls_into_history() {
    let mut parser = vt100_psmux::Parser::new(5, 10, 10);
    parser.process(b"a\r\nb\r\nc\r\nd\r\ne");
    parser.screen_mut().set_size(3, 10);

    assert_eq!(rows(&parser), ["c", "d", "e"]);
    assert_eq!(parser.screen().cursor_position(), (2, 1));
    assert_eq!(parser.screen().scrollback_filled(), 2);

    parser.screen_mut().set_size(5, 10);
    assert_eq!(rows(&parser), ["c", "d", "e", "", ""]);
    assert_eq!(parser.screen().cursor_position(), (2, 1));

    parser.process(b"\r\nf");
    assert_eq!(rows(&parser), ["c", "d", "e", "f", ""]);
}

#[test]
fn height_shrink_without_scrollback_drops_the_top_rows() {
    let mut parser = vt100_psmux::Parser::new(5, 10, 0);
    parser.process(b"a\r\nb\r\nc\r\nd\r\ne");
    parser.screen_mut().set_size(2, 10);

    assert_eq!(rows(&parser), ["d", "e"]);
    assert_eq!(parser.screen().cursor_position(), (1, 1));
    assert_eq!(parser.screen().scrollback_filled(), 0);
}

#[test]
fn height_shrink_keeps_content_anchored_at_the_top() {
    let mut parser = vt100_psmux::Parser::new(5, 10, 10);
    parser.process(b"a\r\nb");
    parser.screen_mut().set_size(3, 10);

    assert_eq!(rows(&parser), ["a", "b", ""]);
    assert_eq!(parser.screen().cursor_position(), (1, 1));
    assert_eq!(parser.screen().scrollback_filled(), 0);
}

#[test]
fn alternate_screen_is_cut_not_reflowed() {
    let mut parser = vt100_psmux::Parser::new(3, 10, 10);
    parser.process(b"0123456789\r\n");
    parser.process(b"\x1b[?1049h");
    parser.process(b"0123456789abc");
    parser.screen_mut().set_size(3, 5);

    assert_eq!(rows(&parser), ["01234", "abc", ""]);
    assert!(!parser.screen().row_wrapped(0));
    assert_eq!(parser.screen().cursor_position(), (1, 3));

    // The main screen underneath was reflowed meanwhile.
    parser.process(b"\x1b[?1049l");
    assert_eq!(rows(&parser), ["01234", "56789", ""]);
    assert!(parser.screen().row_wrapped(0));
}

#[test]
fn wide_chars_are_never_split_across_rows() {
    let mut parser = vt100_psmux::Parser::new(3, 6, 10);
    parser.process("ab\u{4F60}\u{597D}c".as_bytes());
    assert_eq!(parser.screen().cursor_position(), (1, 1));

    parser.screen_mut().set_size(3, 3);
    // "ab" then the glyph moves down rather than straddling the edge; the
    // cursor, just past "c", lands on a row of its own and the top row goes
    // to history.
    assert_eq!(rows(&parser), ["\u{4F60}", "\u{597D}c", ""]);
    for row in 0..2 {
        let cell = parser.screen().cell(row, 0).unwrap();
        assert!(cell.is_wide());
        assert!(parser.screen().cell(row, 1).unwrap().is_wide_continuation());
    }
    assert!(parser.screen().row_wrapped(0));
    assert_eq!(parser.screen().cursor_position(), (2, 0));

    // Growing back drops the pad the moved glyph left behind.
    parser.screen_mut().set_size(3, 6);
    assert_eq!(rows(&parser), ["\u{4F60}\u{597D}c", "", ""]);
    assert_eq!(parser.screen().cursor_position(), (0, 5));
}

#[test]
fn attributes_survive_the_reflow() {
    let mut parser = vt100_psmux::Parser::new(2, 8, 0);
    parser.process(b"ab\x1b[31mcdef\x1b[m");
    parser.screen_mut().set_size(3, 4);

    assert_eq!(rows(&parser), ["abcd", "ef", ""]);
    let red = vt100_psmux::Color::Idx(1);
    assert_eq!(parser.screen().cell(0, 1).unwrap().fgcolor(), vt100_psmux::Color::Default);
    assert_eq!(parser.screen().cell(0, 2).unwrap().fgcolor(), red);
    assert_eq!(parser.screen().cell(1, 1).unwrap().fgcolor(), red);
}
