// `set-option -t <name>` where <name> is no session: tmux's set-option looks
// the target up with CMD_FIND_CANFAIL, so a miss leaves no session, window or
// pane, and options_scope_from_name / options_scope_from_flags refuse with
// "no such session|window|pane: <target>". psmux printed a warning and exited
// 0 from the CLI, and set the option on its own session from a control
// client. Registered from src/cli.rs.

use super::parse_set_option_args;

fn error(args: &[&str]) -> Option<String> {
    parse_set_option_args(args).missing_target_error(false)
}

#[test]
fn session_option_names_the_session() {
    assert_eq!(error(&["-t", "nope", "mouse", "on"]).as_deref(), Some("no such session: nope"));
    assert_eq!(error(&["-t", "nope", "status-left-length", "20"]).as_deref(), Some("no such session: nope"));
}

#[test]
fn window_and_pane_options_name_their_scope() {
    assert_eq!(error(&["-t", "nope", "remain-on-exit", "on"]).as_deref(), Some("no such window: nope"));
    assert_eq!(error(&["-p", "-t", "nope", "remain-on-exit", "on"]).as_deref(), Some("no such pane: nope"));
}

#[test]
fn user_options_follow_the_flags() {
    assert_eq!(error(&["-t", "nope", "@x", "1"]).as_deref(), Some("no such session: nope"));
    assert_eq!(error(&["-w", "-t", "nope", "@x", "1"]).as_deref(), Some("no such window: nope"));
    assert_eq!(parse_set_option_args(&["-t", "nope", "@x", "1"]).missing_target_error(true).as_deref(), Some("no such window: nope"));
    assert_eq!(error(&["-p", "-t", "nope", "@x", "1"]).as_deref(), Some("no such pane: nope"));
    assert_eq!(error(&["-t", "nope:1", "@x", "1"]).as_deref(), Some("no such session: nope:1"), "the whole -t is shown");
}

#[test]
fn options_that_do_not_need_the_target() {
    // A server option, -s, and -g on a session or window option never look at
    // the target's session.
    assert_eq!(error(&["-t", "nope", "escape-time", "0"]), None);
    assert_eq!(error(&["-s", "-t", "nope", "@x", "1"]), None);
    assert_eq!(error(&["-g", "-t", "nope", "mouse", "on"]), None);
    assert_eq!(error(&["-g", "-t", "nope", "@x", "1"]), None);
    assert_eq!(error(&["-g", "-t", "nope", "remain-on-exit", "on"]), None);
    // No -t, or a name tmux cannot match (refused, or ignored under -q, before
    // any scope is looked at).
    assert_eq!(error(&["mouse", "on"]), None);
    assert_eq!(error(&["-q", "-t", "nope", "no-such-option", "1"]), None);
}
