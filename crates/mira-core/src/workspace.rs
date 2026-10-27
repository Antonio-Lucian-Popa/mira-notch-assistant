//! Ephemeral desktop workspace: references only, bounded clipboard and literal grep.
//! Nothing here is serialized to the user's database; clearing never touches originals.
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashSet, VecDeque},
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Link {
    pub path: String,
    pub name: String,
    pub directory: bool,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Workspace {
    pub links: Vec<Link>,
    pub roots: Vec<String>,
    pub clips: VecDeque<String>,
    pub note: String,
    pub capture: bool,
    pub message: String,
}
impl Workspace {
    pub fn new() -> Self {
        Self {
            capture: true,
            ..Self::default()
        }
    }
    pub fn add_paths(&mut self, paths: &[String]) {
        let mut count = 0;
        for path in paths.iter().take(64) {
            let Ok(p) = fs::canonicalize(path) else {
                continue;
            };
            if !p.is_file() && !p.is_dir() {
                continue;
            }
            if p.is_dir() {
                let root = p.to_string_lossy().into_owned();
                if !self.roots.contains(&root) && self.roots.len() < 8 {
                    self.roots.push(root);
                }
                self.link(&p);
                if let Ok(entries) = fs::read_dir(&p) {
                    let mut children: Vec<_> = entries
                        .filter_map(Result::ok)
                        .take(512)
                        .map(|e| e.path())
                        .collect();
                    children.sort();
                    for child in children {
                        if !child.is_symlink() {
                            self.link(&child);
                        }
                    }
                }
            } else {
                self.link(&p);
            }
            count += 1;
        }
        self.message = format!(
            "{count} elemente primite · {} shortcuturi · limita 256",
            self.links.len()
        );
    }
    fn link(&mut self, p: &Path) {
        let path = p.to_string_lossy().into_owned();
        if self.links.len() >= 256 || self.links.iter().any(|l| l.path == path) {
            return;
        }
        self.links.push(Link {
            name: p
                .file_name()
                .unwrap_or(p.as_os_str())
                .to_string_lossy()
                .into_owned(),
            path,
            directory: p.is_dir(),
        });
    }
    pub fn push_clip(&mut self, text: &str) {
        if !self.capture || text.trim().is_empty() || text.trim_start().starts_with("mira-pair://")
        {
            return;
        }
        let text: String = text.chars().take(16_384).collect();
        self.clips.retain(|x| *x != text);
        self.clips.push_front(text);
        self.clips.truncate(32);
    }
    pub fn clear(&mut self) {
        self.links.clear();
        self.roots.clear();
        self.clips.clear();
        self.note.clear();
        self.message.clear();
    }
    pub fn set_note(&mut self, text: &str) {
        self.note = text.chars().take(65_536).collect();
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hit {
    pub path: String,
    pub line: usize,
    pub text: String,
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub hits: Vec<Hit>,
    pub files: usize,
    pub limited: bool,
    pub cancelled: bool,
    pub error: Option<String>,
}
/// Bounded, cancellation-aware literal search. Never follows symlinks or invokes a shell.
pub fn search(roots: &[String], query: &str, generation: &AtomicU64, request: u64) -> SearchResult {
    let mut out = SearchResult::default();
    if query.trim().is_empty() {
        return out;
    }
    if query.len() > 1024 {
        out.error = Some("Căutarea este prea lungă (maxim 1024 bytes).".into());
        return out;
    }
    let needle = match regex::RegexBuilder::new(&regex::escape(query))
        .case_insensitive(true)
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            out.error = Some(e.to_string());
            return out;
        }
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut queue: VecDeque<PathBuf> = roots.iter().take(8).map(PathBuf::from).collect();
    let mut visited = HashSet::new();
    let mut entries = 0;
    while let Some(path) = queue.pop_front() {
        if generation.load(Ordering::Relaxed) != request {
            out.cancelled = true;
            break;
        }
        if out.files >= 4000
            || entries >= 16000
            || out.hits.len() >= 200
            || Instant::now() >= deadline
        {
            out.limited = true;
            break;
        }
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_symlink() {
            continue;
        }
        if meta.is_dir() {
            if let Ok(dir) = fs::read_dir(&path) {
                for item in dir.filter_map(Result::ok).take(4096) {
                    entries += 1;
                    let name = item.file_name();
                    let name = name.to_string_lossy();
                    if name.starts_with('.')
                        || matches!(
                            &*name,
                            "node_modules" | "target" | "build" | "dist" | "vendor" | "__pycache__"
                        )
                    {
                        continue;
                    }
                    if queue.len() < 8192 {
                        queue.push_back(item.path());
                    } else {
                        out.limited = true
                    }
                }
            }
            continue;
        }
        if !meta.is_file() || meta.len() > 2 * 1024 * 1024 || !visited.insert(path.clone()) {
            continue;
        }
        out.files += 1;
        let Ok(file) = fs::File::open(&path) else {
            continue;
        };
        let mut reader = BufReader::new(file);
        let mut buf = Vec::new();
        let mut line = 0;
        loop {
            if generation.load(Ordering::Relaxed) != request {
                out.cancelled = true;
                return out;
            }
            if Instant::now() >= deadline || out.hits.len() >= 200 {
                out.limited = true;
                return out;
            }
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                _ => {}
            }
            line += 1;
            if buf.contains(&0) {
                break;
            }
            let text = String::from_utf8_lossy(&buf);
            if needle.is_match(&text) {
                out.hits.push(Hit {
                    path: path.to_string_lossy().into_owned(),
                    line,
                    text: text.trim().chars().take(240).collect(),
                });
            }
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipboard_is_bounded_deduplicated_and_paused() {
        let mut w = Workspace::new();
        w.push_clip("a");
        w.push_clip("a");
        assert_eq!(w.clips.len(), 1);
        for n in 0..40 {
            w.push_clip(&n.to_string());
        }
        assert_eq!(w.clips.len(), 32);
        w.capture = false;
        w.push_clip("secret");
        assert!(!w.clips.iter().any(|x| x == "secret"));
        w.set_note("scratch");
        w.clear();
        assert!(w.note.is_empty());
        assert!(!w.capture);
    }
    #[test]
    fn folder_links_are_references_and_search_is_literal_cancellable() {
        let dir = std::env::temp_dir().join(format!("mira-workspace-{}", std::process::id()));
        fs::create_dir_all(dir.join("node_modules")).unwrap();
        fs::write(dir.join("one.txt"), "Hello [world]\nsecond line\n").unwrap();
        fs::write(dir.join("node_modules/skip.txt"), "[world]").unwrap();
        fs::write(dir.join("binary"), b"\0[world]").unwrap();
        let mut w = Workspace::new();
        w.add_paths(&[dir.to_string_lossy().into_owned()]);
        assert_eq!(w.roots.len(), 1);
        assert!(w.links.iter().any(|l| l.name == "one.txt"));
        let generation = AtomicU64::new(1);
        let hits = search(&w.roots, "[WORLD]", &generation, 1);
        assert_eq!(hits.hits.len(), 1);
        assert_eq!(hits.hits[0].line, 1);
        assert!(search(&w.roots, "world", &generation, 0).cancelled);
        w.clear();
        assert!(dir.join("one.txt").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
