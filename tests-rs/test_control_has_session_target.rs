// A control client's `has-session -t <name>` answered an empty success block
// whatever the name, because the server holding the connection always has
// its own session. tmux's has-session is the target lookup alone, so a name
// that is not a session is `%error` with "can't find session: <name>".
// Registered from src/server/connection.rs.
//
// Only names that cannot be registered are looked up here: the registry is
// read, never written, and no server is started.

use super::has_session_target_exists;

const MISSING: &str = "psmux-test-no-such-session-7f3a9c";

#[test]
fn this_servers_own_session_exists() {
    for ns in [None, Some("test-ns")] {
        assert!(has_session_target_exists("work", ns, "work"), "{ns:?}");
        assert!(has_session_target_exists("=work", ns, "work"), "exact match marker, {ns:?}");
        assert!(has_session_target_exists("work:1", ns, "work"), "session of a window target, {ns:?}");
        assert!(has_session_target_exists(":1", ns, "work"), "no session part is the current one, {ns:?}");
    }
}

#[test]
fn a_name_with_no_session_does_not_exist() {
    for ns in [None, Some("test-ns")] {
        assert!(!has_session_target_exists(MISSING, ns, "work"), "{ns:?}");
        assert!(!has_session_target_exists(&format!("={MISSING}"), ns, "work"), "{ns:?}");
        assert!(!has_session_target_exists(&format!("{MISSING}:0"), ns, "work"), "{ns:?}");
    }
}

#[test]
fn session_names_are_matched_exactly() {
    assert!(!has_session_target_exists("Work", None, "work"));
    assert!(!has_session_target_exists("wor", None, "work"));
}

#[test]
fn an_unknown_session_id_does_not_exist() {
    assert!(!has_session_target_exists("$987654321", None, "work"));
    assert!(!has_session_target_exists("$987654321", Some("test-ns"), "work"));
}
