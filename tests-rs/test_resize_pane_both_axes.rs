// `resize-pane -x C -y R` applies both sizes, -x first, as tmux's
// cmd-resize-pane.c does. Both routes read the two flags as either or, so the
// -y was dropped whenever -x was given. Registered from
// src/server/connection.rs.

use super::resize_pane_size_requests;
use crate::types::CtrlReq;

fn describe(args: &[&str]) -> Option<Vec<String>> {
    resize_pane_size_requests(args).map(|reqs| {
        reqs.iter()
            .map(|r| match r {
                CtrlReq::ResizePaneAbsolute(axis, n) => format!("{axis}={n}"),
                CtrlReq::ResizePanePercent(axis, p) => format!("{axis}={p}%"),
                _ => "other".to_string(),
            })
            .collect()
    })
}

#[test]
fn x_and_y_together_resize_both_axes() {
    assert_eq!(describe(&["-t", "%1", "-x", "50", "-y", "10"]), Some(vec!["x=50".into(), "y=10".into()]));
    // -x first whatever the order on the command line.
    assert_eq!(describe(&["-y", "10", "-x", "50"]), Some(vec!["x=50".into(), "y=10".into()]));
    assert_eq!(describe(&["-x", "25%", "-y", "50%"]), Some(vec!["x=25%".into(), "y=50%".into()]));
}

#[test]
fn one_axis_alone_is_unchanged() {
    assert_eq!(describe(&["-x", "40"]), Some(vec!["x=40".into()]));
    assert_eq!(describe(&["-y", "8"]), Some(vec!["y=8".into()]));
}

#[test]
fn a_size_flag_with_a_bad_value_is_still_not_a_directional_resize() {
    // As before: a -x that does not parse resizes nothing rather than falling
    // through to the default one cell -D.
    assert_eq!(describe(&["-x", "wide"]), Some(vec![]));
    assert_eq!(describe(&["-x", "wide", "-y", "8"]), Some(vec!["y=8".into()]));
}

#[test]
fn no_size_flag_is_none() {
    assert_eq!(describe(&["-L", "5"]), None);
    assert_eq!(describe(&[]), None);
}
