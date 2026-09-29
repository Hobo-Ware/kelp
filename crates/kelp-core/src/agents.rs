use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Agent {
    Claude,
    Codex,
    Gemini,
    Copilot,
    Aider,
    Cursor,
}

const TABLE: &[(Agent, &str, &[&str])] = &[
    (Agent::Claude, "claude", &["claude-code"]),
    (Agent::Codex, "codex", &[]),
    (Agent::Gemini, "gemini", &["gemini-cli"]),
    (Agent::Copilot, "copilot", &["gh-copilot", "github-copilot"]),
    (Agent::Aider, "aider", &[]),
    (Agent::Cursor, "cursor-agent", &[]),
];

const INTERPRETERS: &[&str] = &[
    "node", "bun", "deno", "python", "python3", "uv", "uvx", "npx",
];

impl Agent {
    pub fn label(self) -> &'static str {
        match self {
            Agent::Claude => "Claude",
            Agent::Codex => "Codex",
            Agent::Gemini => "Gemini",
            Agent::Copilot => "Copilot",
            Agent::Aider => "Aider",
            Agent::Cursor => "Cursor",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase();
        TABLE
            .iter()
            .find(|(_, primary, aliases)| *primary == name || aliases.contains(&name.as_str()))
            .map(|(agent, _, _)| *agent)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Session {
    pub agent: Agent,
    pub pid: u32,
    pub cwd: PathBuf,
}

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn strip_ext(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
}

fn named(segment: &str) -> Option<Agent> {
    Agent::parse(strip_ext(segment))
}

pub fn classify(args: &str) -> Option<Agent> {
    let mut tokens = args.split_whitespace();
    let exe = tokens.next()?;
    if exe.contains(".app/") {
        return None;
    }
    let base = basename(exe);
    if let Some(agent) = Agent::parse(base) {
        return Some(agent);
    }
    if !INTERPRETERS.contains(&base) {
        return None;
    }
    let script = tokens.find(|t| !t.starts_with('-'))?;
    if script.contains(".app/") {
        return None;
    }
    script.split(['/', '\\']).rev().find_map(named)
}

pub fn parse_ps(out: &str) -> Vec<(u32, Agent)> {
    out.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let (pid, args) = line.split_once(char::is_whitespace)?;
            Some((pid.parse().ok()?, classify(args.trim_start())?))
        })
        .collect()
}

pub fn parse_lsof_cwd(out: &str) -> Vec<(u32, PathBuf)> {
    let mut pid = None;
    let mut found = Vec::new();
    for line in out.lines() {
        match line.split_at_checked(1) {
            Some(("p", rest)) => pid = rest.parse().ok(),
            Some(("n", rest)) => {
                if let Some(pid) = pid {
                    found.push((pid, PathBuf::from(rest)));
                }
            }
            _ => {}
        }
    }
    found
}

pub fn sessions() -> Vec<Session> {
    sessions_cached(&mut HashMap::new())
}

pub fn sessions_cached(cwds: &mut HashMap<u32, PathBuf>) -> Vec<Session> {
    let Ok(ps) = Command::new("ps")
        .args(["-axww", "-o", "pid=,args="])
        .output()
    else {
        return Vec::new();
    };
    let candidates = parse_ps(&String::from_utf8_lossy(&ps.stdout));
    cwds.retain(|pid, _| candidates.iter().any(|(p, _)| p == pid));
    let unknown: Vec<String> = candidates
        .iter()
        .filter(|(pid, _)| !cwds.contains_key(pid))
        .map(|(pid, _)| pid.to_string())
        .collect();
    if !unknown.is_empty()
        && let Ok(lsof) = Command::new("lsof")
            .args(["-a", "-d", "cwd", "-Fpn", "-p", &unknown.join(",")])
            .output()
    {
        cwds.extend(parse_lsof_cwd(&String::from_utf8_lossy(&lsof.stdout)));
    }
    candidates
        .into_iter()
        .filter_map(|(pid, agent)| {
            Some(Session {
                agent,
                pid,
                cwd: cwds.get(&pid)?.clone(),
            })
        })
        .collect()
}

pub fn fake_sessions(spec: &str) -> Vec<Session> {
    spec.split(',')
        .filter_map(|part| {
            let (path, agent) = part.rsplit_once(':')?;
            Some(Session {
                agent: Agent::parse(agent)?,
                pid: 0,
                cwd: PathBuf::from(path),
            })
        })
        .collect()
}

pub fn by_worktree(worktrees: &[PathBuf], sessions: &[Session]) -> HashMap<PathBuf, Vec<Agent>> {
    let canonical = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let roots: Vec<(PathBuf, &PathBuf)> = worktrees.iter().map(|w| (canonical(w), w)).collect();
    let mut out: HashMap<PathBuf, Vec<Agent>> = HashMap::new();
    for session in sessions {
        let cwd = canonical(&session.cwd);
        let owner = roots
            .iter()
            .filter(|(root, _)| cwd.starts_with(root))
            .max_by_key(|(root, _)| root.components().count());
        if let Some((_, original)) = owner {
            out.entry((*original).clone())
                .or_default()
                .push(session.agent);
        }
    }
    for agents in out.values_mut() {
        agents.sort();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_binaries_and_interpreter_scripts_are_agents() {
        assert_eq!(classify("claude"), Some(Agent::Claude));
        assert_eq!(
            classify("/opt/homebrew/bin/codex --full-auto"),
            Some(Agent::Codex)
        );
        assert_eq!(
            classify("node /opt/homebrew/lib/node_modules/@google/gemini-cli/dist/index.js"),
            Some(Agent::Gemini)
        );
        assert_eq!(
            classify("node --no-warnings /usr/local/bin/claude --resume"),
            Some(Agent::Claude)
        );
        assert_eq!(
            classify("/usr/bin/python3 /usr/local/bin/aider --model x"),
            Some(Agent::Aider)
        );
        assert_eq!(classify("cursor-agent chat"), Some(Agent::Cursor));
    }

    #[test]
    fn app_bundles_and_look_alikes_are_not() {
        assert_eq!(
            classify(
                "/Applications/Gemini.app/Contents/Helpers/GeminiAppLauncher.app/Contents/MacOS/GeminiAppLauncher"
            ),
            None
        );
        assert_eq!(
            classify("/Applications/Cursor.app/Contents/MacOS/Cursor"),
            None
        );
        assert_eq!(classify("node /Users/me/claude-notes/server.js"), None);
        assert_eq!(classify("vim claude.md"), None);
        assert_eq!(classify("zsh"), None);
        assert_eq!(classify(""), None);
    }

    #[test]
    fn ps_and_lsof_output_are_read_line_by_line() {
        let ps = " 5997 claude\n 6212 /bin/zsh -l\n71206 /Applications/Gemini.app/Contents/MacOS/Gemini\n 7170 node /usr/local/bin/codex\n";
        assert_eq!(
            parse_ps(ps),
            vec![(5997, Agent::Claude), (7170, Agent::Codex)]
        );
        let lsof = "p5997\nfcwd\nn/Users/me/Git/wt-a\np7170\nfcwd\nn/Users/me/Git/wt-b/src\n";
        assert_eq!(
            parse_lsof_cwd(lsof),
            vec![
                (5997, PathBuf::from("/Users/me/Git/wt-a")),
                (7170, PathBuf::from("/Users/me/Git/wt-b/src"))
            ]
        );
    }

    #[test]
    fn a_session_belongs_to_the_deepest_worktree_that_contains_it() {
        let main = PathBuf::from("/repos/app");
        let nested = PathBuf::from("/repos/app/.claude/worktrees/agent-1");
        let sibling = PathBuf::from("/repos/app-feat");
        let session = |agent, cwd: &str| Session {
            agent,
            pid: 1,
            cwd: PathBuf::from(cwd),
        };
        let found = by_worktree(
            &[main.clone(), nested.clone(), sibling.clone()],
            &[
                session(Agent::Claude, "/repos/app/src"),
                session(Agent::Codex, "/repos/app/.claude/worktrees/agent-1/crates"),
                session(Agent::Claude, "/repos/app/.claude/worktrees/agent-1"),
                session(Agent::Aider, "/repos/app-featuring"),
                session(Agent::Gemini, "/somewhere/else"),
            ],
        );
        assert_eq!(found[&main], vec![Agent::Claude]);
        assert_eq!(found[&nested], vec![Agent::Claude, Agent::Codex]);
        assert!(!found.contains_key(&sibling));
    }

    #[test]
    fn fake_sessions_read_path_and_agent_pairs() {
        let fake = fake_sessions("/a/b:claude,/c:codex,broken,/d:nothing");
        assert_eq!(fake.len(), 2);
        assert_eq!(fake[1].agent, Agent::Codex);
    }
}
