// `-e PSMUX_REPLAY_FILE=<path>` seeds a new pane's history with the file's
// bytes. The spawn itself needs ConPTY; these cover the env split, the file
// rules and what the parser holds afterwards.

use super::*;

fn temp_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("psmux-replay-{}-{name}", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    path
}

fn env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[test]
fn replay_key_is_removed_and_file_read() {
    let path = temp_file("read", b"line one\r\nline two\r\n");
    let (kept, replay) = take_replay_file(env(&[
        ("FOO", "1"),
        (REPLAY_FILE_ENV, path.to_str().unwrap()),
        ("BAR", "2"),
    ]));
    let _ = std::fs::remove_file(&path);
    assert_eq!(kept, env(&[("FOO", "1"), ("BAR", "2")]));
    assert_eq!(replay.as_deref(), Some(&b"line one\r\nline two\r\n"[..]));
}

#[test]
fn replay_key_matches_case_insensitively() {
    let path = temp_file("case", b"x");
    let (kept, replay) = take_replay_file(env(&[("psmux_replay_file", path.to_str().unwrap())]));
    let _ = std::fs::remove_file(&path);
    assert!(kept.is_empty());
    assert_eq!(replay.as_deref(), Some(&b"x"[..]));
}

#[test]
fn missing_or_empty_file_is_ignored_but_key_still_removed() {
    let missing = std::env::temp_dir().join("psmux-replay-does-not-exist");
    let (kept, replay) = take_replay_file(env(&[(REPLAY_FILE_ENV, missing.to_str().unwrap())]));
    assert!(kept.is_empty());
    assert!(replay.is_none());

    let empty = temp_file("empty", b"");
    let (_, replay) = take_replay_file(env(&[(REPLAY_FILE_ENV, empty.to_str().unwrap())]));
    let _ = std::fs::remove_file(&empty);
    assert!(replay.is_none());
}

#[test]
fn oversized_file_is_ignored() {
    let big = temp_file("big", &vec![b'a'; REPLAY_FILE_LIMIT as usize + 1]);
    let (_, replay) = take_replay_file(env(&[(REPLAY_FILE_ENV, big.to_str().unwrap())]));
    let _ = std::fs::remove_file(&big);
    assert!(replay.is_none());
}

#[test]
fn no_replay_key_leaves_env_untouched() {
    let input = env(&[("FOO", "1")]);
    let (kept, replay) = take_replay_file(input.clone());
    assert_eq!(kept, input);
    assert!(replay.is_none());
}

/// What the spawn paths do with the bytes: history above the program's output,
/// colours intact, the cursor below the replay.
#[test]
fn replay_becomes_history_above_program_output() {
    let mut parser = vt100::Parser::new(3, 20, 100);
    let mut replay = String::new();
    for i in 0..5 {
        replay.push_str(&format!("old {i}\r\n"));
    }
    replay.push_str("\x1b[31mred\x1b[0m\r\n");
    parser.process(replay.as_bytes());
    parser.process(b"PS> ");

    let screen = parser.screen();
    assert!(screen.scrollback_filled() >= 4, "replay should scroll into history");
    assert_eq!(screen.cursor_position(), (2, 4));
    let rows: Vec<String> = screen.rows(0, 20).collect();
    assert_eq!(rows.last().map(String::as_str), Some("PS> "));
    assert_eq!(rows.get(1).map(String::as_str), Some("red"));
    assert_eq!(screen.cell(1, 0).map(|cell| cell.fgcolor()), Some(vt100::Color::Idx(1)));
}
