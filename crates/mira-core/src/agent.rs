//! AI coding-agent monitoring: the model, the Claude Code hook protocol, and
//! the approval policy. This is the OS-agnostic brain behind Mira's "Agents"
//! page — watch a session live and approve its tool calls from the notch.
//!
//! How it fits together:
//! * A tiny `mira-hook` binary is registered in `~/.claude/settings.json`.
//!   Claude Code runs it for each hook event, passing a JSON payload on stdin.
//! * The hook forwards that payload to the daemon over a local socket. For a
//!   `PreToolUse` event it then waits for a [`Decision`] and prints the matching
//!   Claude Code permission output; for everything else it just reports and exits.
//! * The daemon keeps an [`AgentRegistry`] and asks the UI to decide when a tool
//!   needs approval. If the daemon (or Mira) isn't running the hook fails open,
//!   so Claude Code is never blocked.
//!
//! Everything here is pure and unit-tested; the daemon owns the sockets and the
//! user interaction.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

/// Where a session is in its lifecycle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    #[default]
    Running,
    /// Blocked on a permission the user must grant from the notch.
    Waiting,
    Done,
    Error,
}

/// The nature of a step, so the UI can icon/colour it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepKind {
    Read,
    Edit,
    Run,
    Fetch,
    Notify,
    Prompt,
    #[default]
    Other,
}

/// One line in a session's activity ticker.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentStep {
    pub kind: StepKind,
    pub tool: String,
    pub summary: String,
    #[serde(default)]
    pub added: u32,
    #[serde(default)]
    pub removed: u32,
    pub epoch: i64,
}

/// A tool call waiting for the user's Allow / Deny / Always.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PendingApproval {
    /// Unique id (daemon-assigned) the UI echoes back when responding.
    pub id: String,
    pub session_id: String,
    pub tool: String,
    /// Short label, e.g. "Run command" or "Edit file".
    pub title: String,
    /// The command, file path or detail being approved.
    pub detail: String,
}

/// A single monitored agent session.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    pub agent: String,
    pub cwd: String,
    /// Basename of `cwd`, for a compact title.
    pub project: String,
    pub state: AgentState,
    /// Most recent step summary, for the closed/ticker view.
    pub last_line: String,
    #[serde(default)]
    pub steps: Vec<AgentStep>,
    pub last_activity: i64,
    #[serde(default)]
    pub pending: Option<PendingApproval>,
}

const MAX_STEPS: usize = 40;

/// Tools consequential enough to pause for the user by default. Everything else
/// (reads, searches) is only recorded — Mira never auto-allows what Claude Code
/// would otherwise ask about; it just stays out of the way.
pub const PROMPT_TOOLS: &[&str] = &[
    "Bash",
    "Write",
    "Edit",
    "MultiEdit",
    "NotebookEdit",
    "WebFetch",
];

pub fn needs_prompt(tool: &str) -> bool {
    PROMPT_TOOLS.contains(&tool)
}

// ---------------------------------------------------------------------------
// Hook payload parsing
// ---------------------------------------------------------------------------

/// A decoded Claude Code hook event (the subset Mira reacts to).
#[derive(Clone, Debug, PartialEq)]
pub enum HookEvent {
    SessionStart {
        session_id: String,
        cwd: String,
        agent: String,
    },
    PreToolUse {
        session_id: String,
        cwd: String,
        agent: String,
        tool: String,
        input: Value,
    },
    PostToolUse {
        session_id: String,
        cwd: String,
        agent: String,
        tool: String,
        input: Value,
    },
    Notification {
        session_id: String,
        message: String,
    },
    UserPrompt {
        session_id: String,
        prompt: String,
    },
    Stop {
        session_id: String,
    },
    SessionEnd {
        session_id: String,
    },
    Unknown,
}

fn s(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Agent label: a `coucou_agent`/`mira_agent` tag if present, else "Claude Code".
fn agent_of(v: &Value) -> String {
    for key in ["mira_agent", "coucou_agent", "agent"] {
        let a = s(v, key);
        if !a.is_empty() {
            return a;
        }
    }
    "Claude Code".to_string()
}

/// Parse a raw hook payload. Unknown shapes map to [`HookEvent::Unknown`].
pub fn parse_hook(v: &Value) -> HookEvent {
    let event = s(v, "hook_event_name");
    let session_id = s(v, "session_id");
    let cwd = s(v, "cwd");
    let agent = agent_of(v);
    match event.as_str() {
        "SessionStart" => HookEvent::SessionStart {
            session_id,
            cwd,
            agent,
        },
        "PreToolUse" => HookEvent::PreToolUse {
            session_id,
            cwd,
            agent,
            tool: s(v, "tool_name"),
            input: v.get("tool_input").cloned().unwrap_or(Value::Null),
        },
        "PostToolUse" => HookEvent::PostToolUse {
            session_id,
            cwd,
            agent,
            tool: s(v, "tool_name"),
            input: v.get("tool_input").cloned().unwrap_or(Value::Null),
        },
        "Notification" => HookEvent::Notification {
            session_id,
            message: s(v, "message"),
        },
        "UserPromptSubmit" => HookEvent::UserPrompt {
            session_id,
            prompt: s(v, "prompt"),
        },
        "Stop" | "SubagentStop" => HookEvent::Stop { session_id },
        "SessionEnd" => HookEvent::SessionEnd { session_id },
        _ => HookEvent::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Tool summarization (→ a ticker step)
// ---------------------------------------------------------------------------

fn basename(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

fn trunc(text: &str, n: usize) -> String {
    let t = text.replace(['\n', '\r'], " ");
    let t = t.trim();
    if t.chars().count() > n {
        let cut: String = t.chars().take(n.saturating_sub(1)).collect();
        format!("{cut}…")
    } else {
        t.to_string()
    }
}

fn count_lines(text: &str) -> u32 {
    if text.is_empty() {
        0
    } else {
        text.lines().count().max(1) as u32
    }
}

/// Summarize a tool call into (kind, summary, added, removed).
pub fn summarize_tool(tool: &str, input: &Value) -> (StepKind, String, u32, u32) {
    let path = || basename(&s(input, "file_path"));
    match tool {
        "Bash" => (
            StepKind::Run,
            format!("run: {}", trunc(&s(input, "command"), 72)),
            0,
            0,
        ),
        "Read" => (StepKind::Read, format!("read {}", path()), 0, 0),
        "Grep" => (
            StepKind::Read,
            format!("search “{}”", trunc(&s(input, "pattern"), 40)),
            0,
            0,
        ),
        "Glob" => (
            StepKind::Read,
            format!("glob {}", trunc(&s(input, "pattern"), 40)),
            0,
            0,
        ),
        "LS" => (StepKind::Read, "list files".into(), 0, 0),
        "Write" => {
            let added = count_lines(&s(input, "content"));
            (StepKind::Edit, format!("write {}", path()), added, 0)
        }
        "Edit" => {
            let added = count_lines(&s(input, "new_string"));
            let removed = count_lines(&s(input, "old_string"));
            (StepKind::Edit, format!("edit {}", path()), added, removed)
        }
        "MultiEdit" => {
            let (mut a, mut r) = (0u32, 0u32);
            if let Some(edits) = input.get("edits").and_then(Value::as_array) {
                for e in edits {
                    a += count_lines(&s(e, "new_string"));
                    r += count_lines(&s(e, "old_string"));
                }
            }
            (StepKind::Edit, format!("edit {}", path()), a, r)
        }
        "NotebookEdit" => (
            StepKind::Edit,
            format!("edit {}", basename(&s(input, "notebook_path"))),
            0,
            0,
        ),
        "WebFetch" => (
            StepKind::Fetch,
            format!("fetch {}", trunc(&s(input, "url"), 56)),
            0,
            0,
        ),
        "WebSearch" => (
            StepKind::Fetch,
            format!("search web: {}", trunc(&s(input, "query"), 48)),
            0,
            0,
        ),
        "Task" => (StepKind::Other, "run a sub-agent".into(), 0, 0),
        "TodoWrite" => (StepKind::Other, "update the plan".into(), 0, 0),
        other => (StepKind::Other, other.to_string(), 0, 0),
    }
}

/// A short title + detail for an approval prompt.
pub fn approval_label(tool: &str, input: &Value) -> (String, String) {
    match tool {
        "Bash" => ("Run command".into(), s(input, "command")),
        "Write" => ("Write file".into(), s(input, "file_path")),
        "Edit" | "MultiEdit" => ("Edit file".into(), s(input, "file_path")),
        "NotebookEdit" => ("Edit notebook".into(), s(input, "notebook_path")),
        "WebFetch" => ("Fetch a URL".into(), s(input, "url")),
        other => (other.to_string(), String::new()),
    }
}

// ---------------------------------------------------------------------------
// Decisions + registry
// ---------------------------------------------------------------------------

/// The answer sent back to the hook for a `PreToolUse` event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    /// Bypass the permission system and run the tool.
    Allow,
    /// Block the tool.
    Deny,
    /// Defer to Claude Code's own prompt (the terminal).
    Ask,
    /// No opinion: record only, let normal flow continue.
    Pass,
}

/// What the daemon should do with a `PreToolUse` before replying to the hook.
#[derive(Clone, Debug, PartialEq)]
pub enum PreOutcome {
    /// Reply immediately with this decision.
    Immediate(Decision),
    /// Hold the hook and ask the user; the approval is now on the session.
    Prompt(PendingApproval),
}

/// In-memory set of live sessions plus per-session "always allow" rules.
#[derive(Default)]
pub struct AgentRegistry {
    sessions: Vec<AgentSession>,
    /// (session_id, tool) pairs the user chose to always allow.
    always: HashSet<(String, String)>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sessions(&self) -> &[AgentSession] {
        &self.sessions
    }

    /// Render-ready JSON snapshot for the UI (most-recent session first).
    pub fn snapshot_json(&self) -> Value {
        let mut list: Vec<&AgentSession> = self.sessions.iter().collect();
        list.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
        let waiting = self.sessions.iter().filter(|s| s.pending.is_some()).count();
        json!({
            "sessions": list,
            "waiting": waiting,
            "active": self.sessions.iter().filter(|s| s.state == AgentState::Running || s.state == AgentState::Waiting).count(),
        })
    }

    fn session_mut(&mut self, id: &str, cwd: &str, agent: &str, now: i64) -> &mut AgentSession {
        if let Some(i) = self.sessions.iter().position(|s| s.id == id) {
            let sess = &mut self.sessions[i];
            if !cwd.is_empty() {
                sess.cwd = cwd.to_string();
                sess.project = basename(cwd);
            }
            if !agent.is_empty() {
                sess.agent = agent.to_string();
            }
            sess.last_activity = now;
            return &mut self.sessions[i];
        }
        self.sessions.push(AgentSession {
            id: id.to_string(),
            agent: if agent.is_empty() {
                "Claude Code".into()
            } else {
                agent.to_string()
            },
            cwd: cwd.to_string(),
            project: basename(cwd),
            state: AgentState::Running,
            last_activity: now,
            ..AgentSession::default()
        });
        self.sessions.last_mut().unwrap()
    }

    fn push_step(sess: &mut AgentSession, step: AgentStep) {
        sess.last_line = step.summary.clone();
        sess.steps.push(step);
        if sess.steps.len() > MAX_STEPS {
            let drop = sess.steps.len() - MAX_STEPS;
            sess.steps.drain(0..drop);
        }
    }

    /// Apply a non-blocking event (everything except the `PreToolUse` decision).
    pub fn apply(&mut self, event: &HookEvent, now: i64) {
        match event {
            HookEvent::SessionStart {
                session_id,
                cwd,
                agent,
            } => {
                let sess = self.session_mut(session_id, cwd, agent, now);
                sess.state = AgentState::Running;
            }
            HookEvent::PostToolUse {
                session_id,
                cwd,
                agent,
                tool,
                input,
            } => {
                let (kind, summary, added, removed) = summarize_tool(tool, input);
                let sess = self.session_mut(session_id, cwd, agent, now);
                sess.state = AgentState::Running;
                Self::push_step(
                    sess,
                    AgentStep {
                        kind,
                        tool: tool.clone(),
                        summary,
                        added,
                        removed,
                        epoch: now,
                    },
                );
            }
            HookEvent::Notification {
                session_id,
                message,
            } => {
                let sess = self.session_mut(session_id, "", "", now);
                Self::push_step(
                    sess,
                    AgentStep {
                        kind: StepKind::Notify,
                        tool: "Notification".into(),
                        summary: trunc(message, 80),
                        epoch: now,
                        ..AgentStep::default()
                    },
                );
            }
            HookEvent::UserPrompt { session_id, prompt } => {
                let sess = self.session_mut(session_id, "", "", now);
                sess.state = AgentState::Running;
                Self::push_step(
                    sess,
                    AgentStep {
                        kind: StepKind::Prompt,
                        tool: "Prompt".into(),
                        summary: format!("you: {}", trunc(prompt, 72)),
                        epoch: now,
                        ..AgentStep::default()
                    },
                );
            }
            HookEvent::Stop { session_id } => {
                let sess = self.session_mut(session_id, "", "", now);
                sess.state = AgentState::Done;
                sess.pending = None;
            }
            HookEvent::SessionEnd { session_id } => {
                self.sessions.retain(|s| &s.id != session_id);
            }
            HookEvent::PreToolUse { .. } | HookEvent::Unknown => {}
        }
    }

    /// Decide how to handle a `PreToolUse`. Records the step; returns whether to
    /// reply immediately or to hold for the user. `make_id` mints the approval id.
    pub fn begin_pretooluse(
        &mut self,
        session_id: &str,
        cwd: &str,
        agent: &str,
        tool: &str,
        input: &Value,
        now: i64,
        make_id: impl FnOnce() -> String,
    ) -> PreOutcome {
        let (kind, summary, added, removed) = summarize_tool(tool, input);
        let always = self
            .always
            .contains(&(session_id.to_string(), tool.to_string()));
        {
            let sess = self.session_mut(session_id, cwd, agent, now);
            sess.state = AgentState::Running;
            Self::push_step(
                sess,
                AgentStep {
                    kind,
                    tool: tool.to_string(),
                    summary,
                    added,
                    removed,
                    epoch: now,
                },
            );
        }
        if always {
            return PreOutcome::Immediate(Decision::Allow);
        }
        if !needs_prompt(tool) {
            return PreOutcome::Immediate(Decision::Pass);
        }
        let (title, detail) = approval_label(tool, input);
        let approval = PendingApproval {
            id: make_id(),
            session_id: session_id.to_string(),
            tool: tool.to_string(),
            title,
            detail,
        };
        let sess = self.session_mut(session_id, cwd, agent, now);
        sess.state = AgentState::Waiting;
        sess.pending = Some(approval.clone());
        PreOutcome::Prompt(approval)
    }

    /// Resolve a pending approval by id with the user's choice. `always` records
    /// an allow-rule for that session+tool. Returns the [`Decision`] to send the
    /// hook, or `None` if the id is unknown/stale.
    pub fn resolve(
        &mut self,
        approval_id: &str,
        choice: Decision,
        always: bool,
    ) -> Option<Decision> {
        let idx = self.sessions.iter().position(|s| {
            s.pending
                .as_ref()
                .map(|p| p.id == approval_id)
                .unwrap_or(false)
        })?;
        let sess = &mut self.sessions[idx];
        let tool = sess
            .pending
            .as_ref()
            .map(|p| p.tool.clone())
            .unwrap_or_default();
        sess.pending = None;
        sess.state = AgentState::Running;
        if always && choice == Decision::Allow {
            self.always.insert((sess.id.clone(), tool));
        }
        Some(choice)
    }

    /// First session with a pending approval, for popping the notch open.
    pub fn first_pending(&self) -> Option<&PendingApproval> {
        self.sessions.iter().find_map(|s| s.pending.as_ref())
    }
}

// ---------------------------------------------------------------------------
// settings.json hook installation
// ---------------------------------------------------------------------------

/// Local socket the daemon listens on for `mira-hook` connections (Unix only).
#[cfg(unix)]
pub fn socket_path() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("mira").join("agent.sock")
}

/// Hook events Mira registers `mira-hook` for.
pub const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "PreToolUse",
    "PostToolUse",
    "Notification",
    "UserPromptSubmit",
    "Stop",
    "SessionEnd",
];

/// Produce an updated `settings.json` value with Mira's hooks merged in,
/// preserving the user's existing hooks and never duplicating ours.
/// `timeout_secs` is applied to `PreToolUse` so the user has time to decide.
pub fn merge_hooks(existing: &Value, hook_cmd: &str, timeout_secs: u64) -> Value {
    let mut root = existing.clone();
    if !root.is_object() {
        root = json!({});
    }
    let hooks = root
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let hooks = hooks.as_object_mut().unwrap();
    for event in HOOK_EVENTS {
        let arr = hooks.entry(*event).or_insert_with(|| json!([]));
        if !arr.is_array() {
            *arr = json!([]);
        }
        let arr = arr.as_array_mut().unwrap();
        // Drop any previous Mira entry so re-install is idempotent.
        arr.retain(|group| !group_has_cmd(group, hook_cmd));
        let mut hook = json!({ "type": "command", "command": hook_cmd });
        if *event == "PreToolUse" {
            hook["timeout"] = json!(timeout_secs);
        }
        arr.push(json!({ "matcher": "*", "hooks": [hook] }));
    }
    root
}

fn group_has_cmd(group: &Value, cmd: &str) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .map(|hs| {
            hs.iter()
                .any(|h| h.get("command").and_then(Value::as_str) == Some(cmd))
        })
        .unwrap_or(false)
}

/// Whether `settings.json` already has Mira's hook command installed.
pub fn hooks_installed(existing: &Value, hook_cmd: &str) -> bool {
    existing
        .get("hooks")
        .and_then(Value::as_object)
        .map(|hooks| {
            hooks
                .values()
                .filter_map(Value::as_array)
                .flatten()
                .any(|group| group_has_cmd(group, hook_cmd))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pretooluse() {
        let v = json!({
            "hook_event_name": "PreToolUse",
            "session_id": "s1",
            "cwd": "/home/u/proj",
            "tool_name": "Bash",
            "tool_input": {"command": "ls -la"}
        });
        match parse_hook(&v) {
            HookEvent::PreToolUse {
                session_id,
                tool,
                agent,
                ..
            } => {
                assert_eq!(session_id, "s1");
                assert_eq!(tool, "Bash");
                assert_eq!(agent, "Claude Code");
            }
            other => panic!("wrong: {other:?}"),
        }
    }

    #[test]
    fn summarizes_tools_with_counts() {
        let (k, sum, a, r) = summarize_tool("Bash", &json!({"command": "cargo test"}));
        assert_eq!(k, StepKind::Run);
        assert!(sum.starts_with("run: cargo test"));
        assert_eq!((a, r), (0, 0));

        let (k, _, a, r) = summarize_tool(
            "Edit",
            &json!({"file_path": "/x/y/main.rs", "old_string": "a\nb", "new_string": "a\nb\nc\nd"}),
        );
        assert_eq!(k, StepKind::Edit);
        assert_eq!((a, r), (4, 2));
    }

    #[test]
    fn read_tool_passes_without_prompt() {
        let mut reg = AgentRegistry::new();
        let out = reg.begin_pretooluse(
            "s1",
            "/p",
            "Claude Code",
            "Read",
            &json!({"file_path": "/p/f.rs"}),
            10,
            || "id1".into(),
        );
        assert_eq!(out, PreOutcome::Immediate(Decision::Pass));
        // Step was still recorded.
        assert_eq!(reg.sessions()[0].steps.len(), 1);
        assert!(reg.sessions()[0].pending.is_none());
    }

    #[test]
    fn bash_prompts_then_resolves() {
        let mut reg = AgentRegistry::new();
        let out = reg.begin_pretooluse(
            "s1",
            "/p",
            "Claude Code",
            "Bash",
            &json!({"command": "rm -rf build"}),
            20,
            || "ap1".into(),
        );
        match out {
            PreOutcome::Prompt(a) => {
                assert_eq!(a.id, "ap1");
                assert_eq!(a.title, "Run command");
                assert_eq!(a.detail, "rm -rf build");
            }
            other => panic!("expected prompt: {other:?}"),
        }
        assert_eq!(reg.sessions()[0].state, AgentState::Waiting);
        assert!(reg.first_pending().is_some());

        // Deny it.
        let d = reg.resolve("ap1", Decision::Deny, false);
        assert_eq!(d, Some(Decision::Deny));
        assert!(reg.sessions()[0].pending.is_none());
        assert_eq!(reg.sessions()[0].state, AgentState::Running);
    }

    #[test]
    fn always_allow_is_remembered() {
        let mut reg = AgentRegistry::new();
        reg.begin_pretooluse("s1", "/p", "", "Bash", &json!({"command": "ls"}), 1, || {
            "a1".into()
        });
        assert_eq!(
            reg.resolve("a1", Decision::Allow, true),
            Some(Decision::Allow)
        );
        // Next Bash in the same session is auto-allowed without a prompt.
        let out = reg.begin_pretooluse(
            "s1",
            "/p",
            "",
            "Bash",
            &json!({"command": "pwd"}),
            2,
            || "a2".into(),
        );
        assert_eq!(out, PreOutcome::Immediate(Decision::Allow));
    }

    #[test]
    fn stop_marks_done_and_clears_pending() {
        let mut reg = AgentRegistry::new();
        reg.begin_pretooluse("s1", "/p", "", "Bash", &json!({"command": "x"}), 1, || {
            "a1".into()
        });
        reg.apply(
            &HookEvent::Stop {
                session_id: "s1".into(),
            },
            5,
        );
        assert_eq!(reg.sessions()[0].state, AgentState::Done);
        assert!(reg.sessions()[0].pending.is_none());
    }

    #[test]
    fn resolve_unknown_id_is_none() {
        let mut reg = AgentRegistry::new();
        assert_eq!(reg.resolve("nope", Decision::Allow, false), None);
    }

    #[test]
    fn merge_hooks_is_idempotent_and_preserves_user_hooks() {
        let existing = json!({
            "model": "sonnet",
            "hooks": {
                "PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "/usr/bin/user-hook"}]}]
            }
        });
        let merged = merge_hooks(&existing, "/opt/mira/mira-hook", 600);
        // User's model and their own hook survive.
        assert_eq!(merged["model"], "sonnet");
        let pre = merged["hooks"]["PreToolUse"].as_array().unwrap();
        assert!(pre.iter().any(|g| group_has_cmd(g, "/usr/bin/user-hook")));
        assert!(pre.iter().any(|g| group_has_cmd(g, "/opt/mira/mira-hook")));
        assert!(hooks_installed(&merged, "/opt/mira/mira-hook"));
        // Mira's PreToolUse carries the timeout.
        let mira = pre
            .iter()
            .find(|g| group_has_cmd(g, "/opt/mira/mira-hook"))
            .unwrap();
        assert_eq!(mira["hooks"][0]["timeout"], 600);

        // Re-merging doesn't duplicate Mira's entry.
        let again = merge_hooks(&merged, "/opt/mira/mira-hook", 600);
        let pre2 = again["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(
            pre2.iter()
                .filter(|g| group_has_cmd(g, "/opt/mira/mira-hook"))
                .count(),
            1
        );
    }

    #[test]
    fn merge_hooks_on_empty_settings() {
        let merged = merge_hooks(&Value::Null, "/x/mira-hook", 300);
        assert!(hooks_installed(&merged, "/x/mira-hook"));
        for ev in HOOK_EVENTS {
            assert!(merged["hooks"][ev].is_array());
        }
    }
}
