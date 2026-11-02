//! `mira-hook` — the relay Claude Code runs for each hook event.
//!
//! It reads the hook JSON from stdin, forwards it to the Mira daemon over a
//! local socket, and for a `PreToolUse` event waits for a decision and prints
//! the matching Claude Code permission output. It is **fail-open**: if the
//! daemon (or Mira) isn't running, or anything goes wrong, it exits 0 with no
//! output, so Claude Code's normal flow proceeds and the agent is never blocked.

#[cfg(unix)]
fn main() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    // Read the whole hook payload from stdin.
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() || raw.trim().is_empty() {
        return;
    }
    let payload: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return,
    };
    let event = payload
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Connect to the daemon. Any failure = fail open.
    let path = mira_core::agent::socket_path();
    let mut stream = match UnixStream::connect(&path) {
        Ok(s) => s,
        Err(_) => return,
    };

    let mut line = raw.trim().as_bytes().to_vec();
    line.push(b'\n');
    if stream.write_all(&line).is_err() || stream.flush().is_err() {
        return;
    }

    // Only PreToolUse waits for a decision; everything else is fire-and-forget.
    if event != "PreToolUse" {
        return;
    }

    // Wait for the user, but reply before Claude Code's own hook timeout.
    let _ = stream.set_read_timeout(Some(Duration::from_secs(590)));
    let mut resp = String::new();
    let decision = match stream.read_to_string(&mut resp) {
        Ok(_) => serde_json::from_str::<serde_json::Value>(resp.trim())
            .ok()
            .and_then(|v| v.get("decision").and_then(|d| d.as_str()).map(String::from))
            .unwrap_or_else(|| "ask".into()),
        // Timed out or the daemon went away: fall back to the terminal prompt.
        Err(_) => "ask".into(),
    };

    match decision.as_str() {
        // "pass" = Mira has no opinion: emit nothing, normal flow continues.
        "pass" => {}
        "allow" => emit_decision("allow", "Allowed in Mira"),
        "deny" => emit_decision("deny", "Denied in Mira"),
        _ => emit_decision("ask", "Deferred to the terminal"),
    }

    fn emit_decision(decision: &str, reason: &str) {
        let out = serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": decision,
                "permissionDecisionReason": reason,
            }
        });
        let _ = std::io::stdout().write_all(out.to_string().as_bytes());
    }
}

/// On non-Unix platforms the agent socket is not wired yet (Phase 1 targets
/// Linux/GNOME): behave as a no-op so Claude Code is never blocked.
#[cfg(not(unix))]
fn main() {}
