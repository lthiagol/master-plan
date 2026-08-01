//! Acceptance-criterion verification execution.
//!
//! Classifies each AC's `verification` string and, for runnable ones, executes
//! them to re-check completion.
//!
//! # Trust model
//!
//! `verification` / `tests` strings are repository-controlled plan content.
//! No command executes until the canonical repository has been explicitly
//! trusted. Non-interactive callers fail closed unless trust was persisted
//! previously or `MP_VERIFY_TRUST_REPOSITORY=1` explicitly grants it.
//!
//! Trusted repositories use an argv-only allowlist by default. Set
//! `MP_VERIFY_ALLOW_SHELL=1` to opt into arbitrary shell execution; the exact
//! command is reported before it runs. Trust permits intentional execution but
//! is not a sandbox. `MP_VERIFY_NO_SHELL=1` (or
//! `MP_VERIFY_DEFAULT_NO_SHELL=1`) still disables all automatic execution.
//!
//! Prose-shaped verification strings (parenthetical notes, `+ rg` mid-string
//! clauses, multi-clause `;` prose, multi-`and` English) auto-classify as
//! [`Kind::Manual`] so the complete gate never shell-executes them (M177).
//!
//! Bare integration-test paths (`crates/<pkg>/tests/<name>.rs`) are translated
//! to a cargo-test shell snippet only when `pkg` / suite / filter are safe
//! shell identifiers (`[A-Za-z0-9_-]+`); otherwise the path is not translated
//! and the raw string is left for `sh -c` (or skipped under strict mode).

use std::collections::HashMap;
use std::fs;
use std::io::{self, IsTerminal, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::model::{AcceptanceCriterion, MilestoneFile, Step};

/// M110 (S1): per-`mp milestone complete` cache of already-run commands.
/// Keyed on the literal `verification` / `tests` string (not the translated
/// shell command); stores the first `VerifyOutcome` so cache hits preserve
/// pass/fail. Shared across the AC verifier and step-tests verifier within
/// one complete invocation.
#[derive(Debug, Default)]
pub struct CommandCache(HashMap<String, VerifyOutcome>);

impl CommandCache {
    pub fn new() -> Self {
        Self(HashMap::new())
    }
}

/// M107 (S3): an "always-false" cancel flag for callers that don't
/// explicitly support cancellation (tests, the no-cwd public entry
/// points). Returns a fresh `Arc<AtomicBool>` per call; flipping one
/// never affects another. Wraps `Arc::new` so the call-site signature
/// matches the cancellation-aware entry points.
fn never_cancelled() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn empty_child_pids() -> Arc<Mutex<Vec<u32>>> {
    Arc::new(Mutex::new(Vec::new()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Non-empty, non-`manual:` verification — executed via `sh -c`
    /// (unless `MP_VERIFY_NO_SHELL=1`).
    Runnable,
    /// Prose or `manual:` — not executed.
    Manual,
    /// Empty verification string.
    Empty,
}

const MANUAL_PREFIX: &str = "manual:";
const TRUST_CONFIRM_ENV: &str = "MP_VERIFY_TRUST_REPOSITORY";
const SHELL_OPT_IN_ENV: &str = "MP_VERIFY_ALLOW_SHELL";
const TRUST_STORE_ENV: &str = "MP_VERIFY_TRUST_STORE";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TrustedRepository {
    identity: String,
    canonical_path: String,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct TrustStore {
    #[serde(default)]
    repositories: Vec<TrustedRepository>,
}

#[derive(Debug)]
enum ExecutionMode {
    Argv(Vec<String>),
    Shell(String),
}

const PLUS_TOOLS: &[&str] = &["rg", "grep", "find", "awk", "sed"];

fn env_truthy(name: &str) -> bool {
    matches!(
        std::env::var(name).as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

fn trust_store_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(TRUST_STORE_ENV) {
        return Ok(PathBuf::from(path));
    }
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or_else(|| {
            format!(
                "repository trust is unavailable: set {TRUST_STORE_ENV} or HOME/XDG_CONFIG_HOME"
            )
        })?;
    Ok(config_home
        .join("master-plan")
        .join("trusted-repositories.json"))
}

fn absolute_lexical(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|e| format!("resolve verification working directory: {e}"))
    }
}

/// Reject `..` components in a trust path (lexical only; no symlink walk).
fn reject_parent_dir_components(path: &Path) -> Result<(), String> {
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(format!(
            "repository trust path contains '..': {}",
            path.display()
        ));
    }
    Ok(())
}

/// Refuse a *repository-root alias* that is itself a symlink.
///
/// Trust is keyed on canonical identity (`dev:ino:path`), so platform ancestor
/// rewrites such as macOS `/var` → `/private/var` and `/tmp` → `/private/tmp`
/// are allowed: a project under `/var/folders/...` still reaches the trust
/// prompt/store. What we reject is entering the same repository through a
/// symlink that names the repository root (or the operator-supplied cwd when
/// that cwd is the root alias), so trust is not inherited via an alternate
/// lexical path to the same tree.
fn reject_symlinked_repository_root(path: &Path) -> Result<(), String> {
    let absolute = absolute_lexical(path)?;
    reject_parent_dir_components(&absolute)?;
    let metadata = fs::symlink_metadata(&absolute)
        .map_err(|e| format!("inspect repository path {}: {e}", absolute.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "repository trust is not inherited through symlinked path {}",
            absolute.display()
        ));
    }
    Ok(())
}

fn repository_identity(cwd: Option<&Path>) -> Result<TrustedRepository, String> {
    let requested = cwd
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| "cannot determine verification working directory".to_string())?;
    let requested = absolute_lexical(&requested)?;
    reject_parent_dir_components(&requested)?;

    let root = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&requested)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|root| PathBuf::from(root.trim()))
        .unwrap_or_else(|| requested.clone());
    let root = absolute_lexical(&root)?;
    reject_parent_dir_components(&root)?;

    // No-inheritance: only the repository root path (and the operator cwd when
    // that path itself is a symlink alias) may not be a symlink. Do not walk
    // every absolute prefix — that over-rejects macOS system prefix symlinks
    // (`/var` → `/private/var`, `/tmp` → `/private/tmp`).
    reject_symlinked_repository_root(&root)?;
    if requested != root {
        // When cwd is a symlink *to* the repository root, git may report the
        // canonical toplevel; still refuse the alias used to enter.
        reject_symlinked_repository_root(&requested)?;
    }

    let canonical = root
        .canonicalize()
        .map_err(|e| format!("canonicalize repository root {}: {e}", root.display()))?;
    let metadata = fs::metadata(&canonical)
        .map_err(|e| format!("stat repository root {}: {e}", canonical.display()))?;
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        format!(
            "{}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            canonical.display()
        )
    };
    #[cfg(not(unix))]
    let identity = canonical.to_string_lossy().to_string();

    Ok(TrustedRepository {
        identity,
        canonical_path: canonical.to_string_lossy().to_string(),
    })
}

fn load_trust_store(path: &Path) -> Result<TrustStore, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| format!("parse verification trust store {}: {e}", path.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(TrustStore::default()),
        Err(e) => Err(format!(
            "read verification trust store {}: {e}",
            path.display()
        )),
    }
}

fn persist_repository_trust(
    path: &Path,
    mut store: TrustStore,
    repository: TrustedRepository,
) -> Result<(), String> {
    if !store
        .repositories
        .iter()
        .any(|entry| entry.identity == repository.identity)
    {
        store.repositories.push(repository);
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("trust store {} has no parent", path.display()))?;
    fs::create_dir_all(parent).map_err(|e| {
        format!(
            "create verification trust directory {}: {e}",
            parent.display()
        )
    })?;
    let body = serde_json::to_vec_pretty(&store)
        .map_err(|e| format!("serialize verification trust store: {e}"))?;
    crate::store::atomic_write(path, body)
        .map_err(|e| format!("persist verification trust store {}: {e:#}", path.display()))
}

fn confirm_repository_trust(repository: &TrustedRepository) -> Result<bool, String> {
    if env_truthy(TRUST_CONFIRM_ENV) {
        return Ok(true);
    }
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Ok(false);
    }
    eprintln!(
        "Repository {} requests permission to execute verification commands.",
        repository.canonical_path
    );
    eprintln!(
        "Trust allows intentional local command execution and is NOT a sandbox. Type 'trust' to continue:"
    );
    eprint!("> ");
    io::stderr()
        .flush()
        .map_err(|e| format!("flush trust prompt: {e}"))?;
    let mut response = String::new();
    io::stdin()
        .read_line(&mut response)
        .map_err(|e| format!("read trust confirmation: {e}"))?;
    Ok(response.trim() == "trust")
}

fn require_repository_trust(cwd: Option<&Path>) -> Result<TrustedRepository, String> {
    let repository = repository_identity(cwd)?;
    let store_path = trust_store_path()?;
    let store = load_trust_store(&store_path)?;
    if store
        .repositories
        .iter()
        .any(|entry| entry.identity == repository.identity)
    {
        return Ok(repository);
    }
    if !confirm_repository_trust(&repository)? {
        return Err(format!(
            "repository {} is not trusted; no verification command was executed. \
             Re-run interactively and type 'trust', or explicitly set {TRUST_CONFIRM_ENV}=1",
            repository.canonical_path
        ));
    }
    persist_repository_trust(&store_path, store, repository.clone())?;
    Ok(repository)
}

fn is_shell_operator(c: char) -> bool {
    matches!(
        c,
        ';' | '|' | '&' | '<' | '>' | '$' | '`' | '(' | ')' | '\n' | '\r' | '\0'
    )
}

/// Parse the safe subset needed by standard test commands. Shell operators,
/// substitutions, redirections, and unterminated quotes are rejected before
/// any process is spawned.
fn parse_argv(command: &str) -> Result<Vec<String>, String> {
    let mut argv = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    let mut quote: Option<char> = None;
    let mut started = false;

    while let Some(c) = chars.next() {
        match quote {
            Some('\'') => {
                if c == '\'' {
                    quote = None;
                } else {
                    current.push(c);
                }
            }
            Some('"') => {
                if c == '"' {
                    quote = None;
                } else if c == '\\' {
                    let escaped = chars
                        .next()
                        .ok_or_else(|| "trailing backslash in double quotes".to_string())?;
                    if matches!(escaped, '"' | '\\') {
                        current.push(escaped);
                    } else {
                        return Err(format!(
                            "unsupported escape \\{escaped} in argv-only verification"
                        ));
                    }
                } else if is_shell_operator(c) {
                    return Err(format!(
                        "shell expansion/operator {c:?} is not allowed in argv-only verification"
                    ));
                } else {
                    current.push(c);
                }
            }
            None if c.is_whitespace() => {
                if started {
                    argv.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                started = true;
            }
            None if c == '\\' => {
                let escaped = chars
                    .next()
                    .ok_or_else(|| "trailing backslash in argv-only verification".to_string())?;
                if escaped == '\n' || escaped == '\r' {
                    return Err("line continuation is not allowed in argv-only verification".into());
                }
                current.push(escaped);
                started = true;
            }
            None if is_shell_operator(c) => {
                return Err(format!(
                    "shell operator {c:?} is not allowed in argv-only verification"
                ));
            }
            None => {
                current.push(c);
                started = true;
            }
            Some(_) => unreachable!("quote state is limited to single/double quote"),
        }
    }
    if quote.is_some() {
        return Err("unterminated quote in argv-only verification".into());
    }
    if started {
        argv.push(current);
    }
    if argv.is_empty() {
        return Err("empty argv-only verification".into());
    }
    Ok(argv)
}

fn argv_tool_allowed(tool: &str) -> bool {
    matches!(
        tool,
        "cargo"
            | "make"
            | "mp"
            | "raul"
            | "rg"
            | "grep"
            | "git"
            | "npm"
            | "node"
            | "python"
            | "python3"
            | "test"
            | "true"
            | "false"
            | "echo"
            | "printf"
    ) || tool
        .strip_prefix("./scripts/")
        .is_some_and(|rest| !rest.is_empty() && !rest.contains('/'))
}

fn execution_mode(command: &str, cwd: Option<&Path>) -> Result<ExecutionMode, String> {
    // The no-cwd API is an explicit embedding surface: its caller supplied the
    // command directly and deliberately omitted repository context. Production
    // plan execution always passes `Some(project_root)` and therefore cannot
    // reach this compatibility path.
    if cwd.is_none() {
        return Ok(ExecutionMode::Shell(command.to_string()));
    }
    let repository = require_repository_trust(cwd)?;
    if env_truthy(SHELL_OPT_IN_ENV) {
        eprintln!(
            "trusted shell verification in {} (not sandboxed): {}",
            repository.canonical_path, command
        );
        return Ok(ExecutionMode::Shell(command.to_string()));
    }
    let argv = parse_argv(command)?;
    if !argv_tool_allowed(&argv[0]) {
        return Err(format!(
            "argv-only verification rejects executable {:?}; set {SHELL_OPT_IN_ENV}=1 \
             after reviewing the exact command to opt into trusted shell mode",
            argv[0]
        ));
    }
    Ok(ExecutionMode::Argv(argv))
}

fn verify_no_shell() -> bool {
    env_truthy("MP_VERIFY_NO_SHELL") || env_truthy("MP_VERIFY_DEFAULT_NO_SHELL")
}

/// Classify a verification string, honoring `MP_VERIFY_NO_SHELL` /
/// `MP_VERIFY_DEFAULT_NO_SHELL`.
///
/// Prefer [`classify_with`] in unit tests so parallel suites do not race on
/// process-global env mutation.
pub fn classify(verification: &str) -> Kind {
    classify_with(verification, verify_no_shell())
}

/// Pure classification: `no_shell` is injected (no env I/O).
///
/// Order: empty → `manual:` prefix → prose detector → `no_shell` → Runnable.
/// The `manual:` prefix always wins over prose detection.
pub fn classify_with(verification: &str, no_shell: bool) -> Kind {
    let trimmed = verification.trim();
    if trimmed.is_empty() {
        return Kind::Empty;
    }
    if trimmed.to_ascii_lowercase().starts_with(MANUAL_PREFIX) {
        return Kind::Manual;
    }
    if looks_like_prose(trimmed) {
        return Kind::Manual;
    }
    if no_shell {
        return Kind::Manual;
    }
    Kind::Runnable
}

/// True when `verification` is descriptive prose rather than a shell command.
///
/// Patterns (M177): parenthetical notes outside recognised command forms,
/// mid-string `+ {rg,grep,find,awk,sed}`, `;`-separated non-command clauses,
/// and two-or-more English `and` conjunctions.
pub fn looks_like_prose(verification: &str) -> bool {
    let s = verification.trim();
    if s.is_empty() {
        return false;
    }
    if has_plus_tool_clause(s) {
        return true;
    }
    if has_prose_parens(s) {
        return true;
    }
    if has_prose_semicolons(s) {
        return true;
    }
    if count_and_conjunctions(s) >= 2 {
        return true;
    }
    false
}

fn has_plus_tool_clause(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    for tool in PLUS_TOOLS {
        let pat = format!(" + {tool}");
        let mut rest = lower.as_str();
        while let Some(idx) = rest.find(&pat) {
            let after = idx + pat.len();
            let boundary_ok =
                after >= rest.len() || !rest.as_bytes()[after].is_ascii_alphanumeric();
            if boundary_ok {
                return true;
            }
            rest = &rest[idx + 1..];
        }
    }
    false
}

/// Parenthetical prose notes outside quotes (e.g. `file.rs (grep-based test)`).
/// Quote-aware (mirrors [`split_semicolon_clauses`]): parens inside `'…'` /
/// `"…"` are ignored so `cargo nextest -E 'test(/foo-bar/)'` stays Runnable
/// (M177 external F-07). Bare `-` inside the parenthetical is not a prose
/// signal — documented cases always include a space (`(grep-based test)`).
fn has_prose_parens(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut in_single = false;
    let mut in_double = false;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if b == b'\'' && !in_double {
            in_single = !in_single;
            i += 1;
            continue;
        }
        if b == b'"' && !in_single {
            in_double = !in_double;
            i += 1;
            continue;
        }
        if in_single || in_double {
            i += 1;
            continue;
        }
        if b == b'(' {
            let before = s[..i].trim_end();
            let at_start = before.is_empty();
            let is_cmd_sub = i > 0 && bytes[i - 1] == b'$';
            if at_start || is_cmd_sub {
                i += 1;
                continue;
            }
            match s[i + 1..].find(')') {
                Some(rel_end) => {
                    let inner = &s[i + 1..i + 1 + rel_end];
                    // Space inside the parenthetical is the prose signal
                    // (`(grep-based test)`, `(full plan)`). Hyphen alone is
                    // not — it false-positives nextest filters and shell
                    // tokens when quote-blind (F-07).
                    if inner.contains(' ') {
                        return true;
                    }
                    i += 1 + rel_end + 1;
                    continue;
                }
                None => return true,
            }
        }
        i += 1;
    }
    false
}

fn has_prose_semicolons(s: &str) -> bool {
    let segments = split_semicolon_clauses(s);
    if segments.len() < 2 {
        return false;
    }
    segments
        .into_iter()
        .map(|seg| seg.trim())
        .filter(|seg| !seg.is_empty())
        .any(|seg| !looks_like_command_start(seg))
}

/// Split on `"; "` that are outside single/double quotes (backslash-escape
/// aware). Avoids treating `awk '{f=1; next}'` as multi-clause prose.
fn split_semicolon_clauses(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if b == b'\'' && !in_double {
            in_single = !in_single;
            i += 1;
            continue;
        }
        if b == b'"' && !in_single {
            in_double = !in_double;
            i += 1;
            continue;
        }
        if !in_single && !in_double && b == b';' && i + 1 < bytes.len() && bytes[i + 1] == b' ' {
            out.push(&s[start..i]);
            start = i + 2;
            i += 2;
            continue;
        }
        i += 1;
    }
    out.push(&s[start..]);
    out
}

fn looks_like_command_start(s: &str) -> bool {
    let mut rest = s.trim();
    // Transparent leading `!` (history/negation) and `VAR=value` prefixes.
    loop {
        if let Some(stripped) = rest.strip_prefix('!') {
            rest = stripped.trim_start();
            continue;
        }
        if let Some(eq) = rest.find('=') {
            let key = &rest[..eq];
            if !key.is_empty()
                && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && key
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            {
                if let Some(after) = rest[eq + 1..].find(char::is_whitespace) {
                    rest = rest[eq + 1 + after..].trim_start();
                    continue;
                }
            }
        }
        break;
    }
    let first = match rest.split_whitespace().next() {
        Some(t) if !t.is_empty() => t,
        _ => return false,
    };
    if first.starts_with("./")
        || first.starts_with('/')
        || first.contains('/')
        || first.ends_with(".sh")
        || first.ends_with(".rs")
    {
        return true;
    }
    matches!(
        first,
        "cargo"
            | "make"
            | "mp"
            | "raul"
            | "bash"
            | "sh"
            | "zsh"
            | "rg"
            | "grep"
            | "find"
            | "awk"
            | "sed"
            | "git"
            | "npm"
            | "python"
            | "python3"
            | "node"
            | "eval"
            | "cd"
            | "test"
            | "true"
            | "false"
            | "echo"
            | "printf"
            | "xargs"
            | "wc"
            | "cat"
            | "ls"
            | "env"
            | "timeout"
            | "set"
            | "export"
            | "unset"
            | "pushd"
            | "popd"
            // Shell control-flow keywords so `for i in …; do …; done` stays
            // Runnable under the semicolon-clause heuristic.
            | "for"
            | "do"
            | "done"
            | "if"
            | "then"
            | "else"
            | "elif"
            | "fi"
            | "while"
            | "until"
            | "case"
            | "esac"
            | "in"
    ) || first.starts_with("cargo-")
}

fn count_and_conjunctions(s: &str) -> usize {
    let lower = s.to_ascii_lowercase();
    let mut count = 0;
    let mut rest = lower.as_str();
    while let Some(idx) = rest.find(" and ") {
        count += 1;
        rest = &rest[idx + 5..];
    }
    count
}

/// Shell command to run for a runnable verification/tests value.
pub fn command_for_execution(verification: &str) -> String {
    let trimmed = verification.trim();
    if let Some(cmd) = translate_bare_rs_test_path(trimmed) {
        return cmd;
    }
    rewrite_legacy_cargo_test_invocations(trimmed)
}

/// Map a legacy per-file integration test name to its consolidated suite binary.
pub fn resolve_integration_test_binary(crate_name: &str, legacy_test_name: &str) -> String {
    if crate_name == "mp" {
        if let Some((_, suite)) = crate::integration_test_map::LEGACY_TEST_BINARY_MAP
            .iter()
            .find(|(legacy, _)| *legacy == legacy_test_name)
        {
            return (*suite).to_string();
        }
    }
    legacy_test_name.to_string()
}

/// Rewrite `cargo test -p mp --test <legacy>` (and nextest equivalents) in
/// compound shell commands.
///
/// Scoped to package `mp` only. Other crates may still ship real binaries
/// whose names collide with [`crate::integration_test_map::LEGACY_TEST_BINARY_MAP`]
/// keys — `-p mp-oracle --test mini_schema_parity` must not become
/// `--test suite_validate`.
fn rewrite_legacy_cargo_test_invocations(cmd: &str) -> String {
    let mut entries: Vec<_> = crate::integration_test_map::LEGACY_TEST_BINARY_MAP.to_vec();
    entries.sort_by_key(|(legacy, _)| std::cmp::Reverse(legacy.len()));
    let mut out = cmd.to_string();
    for (legacy, suite) in entries {
        out = rewrite_mp_package_test_flag(&out, legacy, suite);
    }
    out
}

/// Replace `--test <legacy>` with `--test <suite> <legacy>::` only when the
/// enclosing shell segment targets `-p mp` / `--package mp`.
fn rewrite_mp_package_test_flag(cmd: &str, legacy: &str, suite: &str) -> String {
    let needle = format!("--test {legacy}");
    let replacement = format!("--test {suite} {legacy}::");
    let mut result = String::with_capacity(cmd.len());
    let mut rest = cmd;
    while let Some(idx) = rest.find(&needle) {
        let after = idx + needle.len();
        // End-of-token: avoid rewriting a prefix of a longer `--test` name.
        let boundary_ok = rest[after..]
            .chars()
            .next()
            .map(|c| c.is_ascii_whitespace() || c == '"' || c == '\'')
            .unwrap_or(true);
        let before = &rest[..idx];
        let after_part = &rest[after..];
        // Include flags after `--test` in the same shell segment so
        // `cargo test --test foo -p mp` still resolves package `mp`.
        let seg_prefix = last_shell_command_segment(before);
        let seg_suffix_len = shell_segment_prefix_len(after_part);
        let mut segment_for_pkg = String::with_capacity(seg_prefix.len() + seg_suffix_len);
        segment_for_pkg.push_str(seg_prefix);
        segment_for_pkg.push_str(&after_part[..seg_suffix_len]);
        result.push_str(before);
        if boundary_ok && extract_cargo_package(&segment_for_pkg).as_deref() == Some("mp") {
            result.push_str(&replacement);
        } else {
            result.push_str(&needle);
        }
        rest = &rest[after..];
    }
    result.push_str(rest);
    result
}

/// Slice from the last `&&`, `||`, `|`, `;`, or newline to the end.
fn last_shell_command_segment(s: &str) -> &str {
    let bytes = s.as_bytes();
    let mut last_break = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' | b'\r' | b';' => {
                last_break = i + 1;
                i += 1;
            }
            b'&' if i + 1 < bytes.len() && bytes[i + 1] == b'&' => {
                last_break = i + 2;
                i += 2;
            }
            b'|' if i + 1 < bytes.len() && bytes[i + 1] == b'|' => {
                last_break = i + 2;
                i += 2;
            }
            b'|' => {
                last_break = i + 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    s[last_break..].trim_start()
}

/// Bytes until the next shell command break (`&&`, `||`, `|`, `;`, newline).
fn shell_segment_prefix_len(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' | b'\r' | b';' => return i,
            b'&' if i + 1 < bytes.len() && bytes[i + 1] == b'&' => return i,
            b'|' => return i,
            _ => i += 1,
        }
    }
    s.len()
}

/// Last `-p` / `--package` / `--package=` value in a cargo/nextest segment.
fn extract_cargo_package(segment: &str) -> Option<String> {
    let parts: Vec<&str> = segment.split_whitespace().collect();
    let mut found = None;
    let mut i = 0usize;
    while i < parts.len() {
        let p = parts[i];
        if p == "-p" || p == "--package" {
            if let Some(pkg) = parts.get(i + 1) {
                found = Some((*pkg).to_string());
                i += 2;
                continue;
            }
        } else if let Some(pkg) = p.strip_prefix("--package=") {
            if !pkg.is_empty() {
                found = Some(pkg.to_string());
            }
        } else if let Some(pkg) = p.strip_prefix("-p") {
            // Glued short form (`-pmp`); skip bare `-p` (handled above).
            if !pkg.is_empty() {
                found = Some(pkg.to_string());
            }
        }
        i += 1;
    }
    found
}

fn cargo_test_invocation_shell(
    pkg: &str,
    test_binary: &str,
    module_filter: Option<&str>,
) -> String {
    let filter_sp = module_filter
        .filter(|f| !f.is_empty())
        .map(|f| format!(" {f}"))
        .unwrap_or_default();
    // When a test binary only has `#[ignore]` tests, plain `cargo test` exits 0 with
    // 0 passed — re-run with `--include-ignored` so guardrail fixtures still gate.
    format!(
        r#"out=$(cargo test -p {pkg} --test {test_binary}{filter_sp} 2>&1); ec=$?; printf '%s\n' "$out"; \
if [ $ec -eq 0 ] && printf '%s' "$out" | grep -qE 'test result: ok\. 0 passed; 0 failed; [1-9][0-9]* ignored'; then \
  cargo test -p {pkg} --test {test_binary}{filter_sp} -- --include-ignored; \
else exit $ec; fi"#
    )
}

/// Safe for interpolation into the cargo-test shell snippet: alphanumeric, `_`, `-`.
fn is_safe_shell_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// `crates/<pkg>/tests/<name>.rs` → `cargo test -p <pkg> --test <suite>`
fn translate_bare_rs_test_path(s: &str) -> Option<String> {
    // Reject shell metacharacters / expansion before any interpolation.
    if s.chars().any(|c| {
        matches!(
            c,
            ';' | '|'
                | '&'
                | '$'
                | '`'
                | '('
                | ')'
                | '\n'
                | '\r'
                | '"'
                | '\''
                | '<'
                | '>'
                | '\\'
                | '{'
                | '}'
                | '['
                | ']'
                | '*'
                | '?'
                | '~'
                | '#'
                | '!'
                | '\0'
        )
    }) {
        return None;
    }
    let path = s.strip_prefix("./").unwrap_or(s);
    if !path.ends_with(".rs") {
        return None;
    }
    if s.split_whitespace().count() != 1 {
        return None;
    }
    let parts: Vec<&str> = path.split('/').collect();
    let crates_idx = parts.iter().position(|p| *p == "crates")?;
    let pkg = parts.get(crates_idx + 1)?;
    let tests_idx = parts.iter().position(|p| *p == "tests")?;
    if tests_idx <= crates_idx + 1 {
        return None;
    }
    let file = parts.last()?;
    let legacy = file.strip_suffix(".rs")?;
    if !is_safe_shell_ident(pkg) || !is_safe_shell_ident(legacy) {
        return None;
    }
    // Intermediate path segments must also be safe idents (no `..`, empty, etc.).
    for (i, part) in parts.iter().enumerate() {
        if i == crates_idx {
            continue;
        }
        if *part == "tests" || *part == "crates" {
            continue;
        }
        if i == parts.len() - 1 {
            continue; // file name checked via legacy
        }
        if !is_safe_shell_ident(part) {
            return None;
        }
    }
    let suite = resolve_integration_test_binary(pkg, legacy);
    if !is_safe_shell_ident(&suite) {
        return None;
    }
    let filter = if suite == legacy {
        None
    } else {
        Some(format!("{legacy}::"))
    };
    Some(cargo_test_invocation_shell(pkg, &suite, filter.as_deref()))
}

#[derive(Debug, Clone)]
struct VerifyOutcome {
    kind: Kind,
    passed: bool,
    exit_code: Option<i32>,
    output: String,
    note: String,
}

struct VerifyLabels {
    empty_note: &'static str,
    manual_note: &'static str,
    command_kind: &'static str,
}

const AC_LABELS: VerifyLabels = VerifyLabels {
    empty_note: "empty verification — not checked (flagged)",
    manual_note: "manual verification — not executed",
    command_kind: "verification",
};

const STEP_LABELS: VerifyLabels = VerifyLabels {
    empty_note: "empty tests — not checked",
    manual_note: "manual tests — not executed",
    command_kind: "step tests",
};

/// M106 (S4): the shared runner for both AC and step verifications.
/// `run_one_in` (AC verifier) and `run_step_test_in` (step verifier)
/// both delegate here. Owns: classify + (empty/manual/runnable) arms +
/// execute-with-pipe-drain + truncate-for-display. Envelope mapping
/// from `VerifyOutcome` to `AcResult` / `StepTestsResult` is the only
/// per-callsite difference; the rest is shared here.
///
/// M107 (S3): `cancelled` and `child_pids` are threaded so `execute`
/// can register the child's pgid (= pid, set via `process_group(0)`)
/// into the orchestrator's registry for process-group kill on global-
/// deadline, and so it can observe the cooperative cancel flag between
/// `try_wait()` polls.
fn run_one(
    value: &str,
    cwd: Option<&std::path::Path>,
    labels: VerifyLabels,
    cancelled: &Arc<AtomicBool>,
    child_pids: &Arc<Mutex<Vec<u32>>>,
    cache: Option<&mut CommandCache>,
) -> VerifyOutcome {
    let kind = classify(value);
    match kind {
        Kind::Empty => VerifyOutcome {
            kind,
            passed: true,
            exit_code: None,
            output: String::new(),
            note: labels.empty_note.to_string(),
        },
        Kind::Manual => VerifyOutcome {
            kind,
            passed: true,
            exit_code: None,
            output: String::new(),
            note: labels.manual_note.to_string(),
        },
        Kind::Runnable => {
            if let Some(cached) = cache.as_ref().and_then(|c| c.0.get(value)).cloned() {
                let mut outcome = cached;
                outcome.note = format!("{} command skipped (cache hit)", labels.command_kind);
                return outcome;
            }
            let command = command_for_execution(value);
            let outcome = match execution_mode(&command, cwd)
                .and_then(|mode| execute(mode, cwd, cancelled, child_pids))
            {
                Ok((code, out)) => VerifyOutcome {
                    kind,
                    passed: code == 0,
                    exit_code: Some(code),
                    output: truncate(out, 2000),
                    note: if code == 0 {
                        format!("{} command succeeded", labels.command_kind)
                    } else {
                        format!("{} command failed (exit {code})", labels.command_kind)
                    },
                },
                Err(e) => VerifyOutcome {
                    kind,
                    passed: false,
                    exit_code: None,
                    output: String::new(),
                    note: format!("{} command did not run: {e}", labels.command_kind),
                },
            };
            if let Some(cache) = cache {
                cache.0.insert(value.to_string(), outcome.clone());
            }
            outcome
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AcResult {
    pub ac_id: String,
    pub description: String,
    pub verification: String,
    pub kind: Kind,
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub output: String,
    pub note: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VerifyReport {
    pub milestone: String,
    pub display: String,
    pub title: String,
    pub results: Vec<AcResult>,
    pub runnable_total: usize,
    pub runnable_passed: usize,
    pub runnable_failed: usize,
    pub manual: usize,
    pub empty: usize,
    /// true iff no runnable AC failed (manual/empty never block).
    pub ok: bool,
}

impl VerifyReport {
    pub fn failures(&self) -> Vec<&AcResult> {
        self.results
            .iter()
            .filter(|r| r.kind == Kind::Runnable && !r.passed)
            .collect()
    }
}

pub fn run_one_default(ac: &AcceptanceCriterion) -> AcResult {
    run_one_in(ac, None, &never_cancelled(), &empty_child_pids(), None)
}

pub fn run_one_in(
    ac: &AcceptanceCriterion,
    cwd: Option<&std::path::Path>,
    cancelled: &Arc<AtomicBool>,
    child_pids: &Arc<Mutex<Vec<u32>>>,
    cache: Option<&mut CommandCache>,
) -> AcResult {
    let outcome = run_one(
        &ac.verification,
        cwd,
        AC_LABELS,
        cancelled,
        child_pids,
        cache,
    );
    AcResult {
        ac_id: ac.id.clone(),
        description: ac.description.clone(),
        verification: ac.verification.clone(),
        kind: outcome.kind,
        passed: outcome.passed,
        exit_code: outcome.exit_code,
        output: outcome.output,
        note: outcome.note,
    }
}

pub fn verify_milestone(m: &MilestoneFile) -> VerifyReport {
    verify_milestone_in(m, None, &never_cancelled(), &empty_child_pids(), None)
}

/// M106 (S5): AC verifier delegates per-AC execution to `run_one_in`
/// which calls the shared `run_one` runner (S4) and builds an `AcResult`
/// envelope from the `VerifyOutcome`. Same one-runner-one-classifier
/// pattern as `verify_step_tests_in`.
///
/// M107 (S3): `cancelled` and `child_pids` are passed through every
/// layer so a global-deadline handler can flip the flag, the worker can
/// observe it between `try_wait()` polls, and `execute` can register the
/// `sh` child pid into `child_pids` so the orchestrator can `killpg`
/// the whole process tree if the cooperative flag is ignored.
pub fn verify_milestone_in(
    m: &MilestoneFile,
    cwd: Option<&std::path::Path>,
    cancelled: &Arc<AtomicBool>,
    child_pids: &Arc<Mutex<Vec<u32>>>,
    mut cache: Option<&mut CommandCache>,
) -> VerifyReport {
    let results: Vec<AcResult> = m
        .acceptance_criteria
        .iter()
        .map(|ac| run_one_in(ac, cwd, cancelled, child_pids, cache.as_deref_mut()))
        .collect();
    let counts = summarize_kinds(&results, |r| r.kind, |r| r.passed);
    VerifyReport {
        milestone: m.milestone.id.clone(),
        display: crate::paths::display_milestone_id(&m.milestone.id),
        title: m.milestone.title.clone(),
        ok: counts.runnable_failed == 0,
        results,
        runnable_total: counts.runnable_total,
        runnable_passed: counts.runnable_passed,
        runnable_failed: counts.runnable_failed,
        manual: counts.manual,
        empty: counts.empty,
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StepTestsResult {
    pub step_id: String,
    pub tests: String,
    pub kind: Kind,
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub output: String,
    pub note: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StepTestsReport {
    pub milestone: String,
    pub display: String,
    pub title: String,
    pub results: Vec<StepTestsResult>,
    pub runnable_total: usize,
    pub runnable_passed: usize,
    pub runnable_failed: usize,
    pub manual: usize,
    pub empty: usize,
    pub ok: bool,
}

pub fn run_step_test(step: &Step) -> StepTestsResult {
    run_step_test_in(step, None, &never_cancelled(), &empty_child_pids(), None)
}

pub fn run_step_test_in(
    step: &Step,
    cwd: Option<&std::path::Path>,
    cancelled: &Arc<AtomicBool>,
    child_pids: &Arc<Mutex<Vec<u32>>>,
    cache: Option<&mut CommandCache>,
) -> StepTestsResult {
    let outcome = run_one(&step.tests, cwd, STEP_LABELS, cancelled, child_pids, cache);
    StepTestsResult {
        step_id: step.id.clone(),
        tests: step.tests.clone(),
        kind: outcome.kind,
        passed: outcome.passed,
        exit_code: outcome.exit_code,
        output: outcome.output,
        note: outcome.note,
    }
}

pub fn verify_step_tests(m: &MilestoneFile) -> StepTestsReport {
    verify_step_tests_in(m, None, &never_cancelled(), &empty_child_pids(), None)
}

/// M106 (S5): step-tests verifier mirrors `verify_milestone_in` —
/// same shared `run_one` runner, parallel report construction. Per-step
/// envelope (`StepTestsResult`) is the only shape difference from
/// `AcResult`; the underlying `VerifyOutcome` is identical.
///
/// M107 (S3): see `verify_milestone_in` for cancel/child-pid plumbing.
pub fn verify_step_tests_in(
    m: &MilestoneFile,
    cwd: Option<&std::path::Path>,
    cancelled: &Arc<AtomicBool>,
    child_pids: &Arc<Mutex<Vec<u32>>>,
    mut cache: Option<&mut CommandCache>,
) -> StepTestsReport {
    let results: Vec<StepTestsResult> = m
        .steps
        .iter()
        .map(|s| run_step_test_in(s, cwd, cancelled, child_pids, cache.as_deref_mut()))
        .collect();
    let counts = summarize_kinds(&results, |r| r.kind, |r| r.passed);
    StepTestsReport {
        milestone: m.milestone.id.clone(),
        display: crate::paths::display_milestone_id(&m.milestone.id),
        title: m.milestone.title.clone(),
        ok: counts.runnable_failed == 0,
        results,
        runnable_total: counts.runnable_total,
        runnable_passed: counts.runnable_passed,
        runnable_failed: counts.runnable_failed,
        manual: counts.manual,
        empty: counts.empty,
    }
}

struct KindCounts {
    runnable_total: usize,
    runnable_passed: usize,
    runnable_failed: usize,
    manual: usize,
    empty: usize,
}

fn summarize_kinds<T>(
    results: &[T],
    kind_of: impl Fn(&T) -> Kind,
    passed_of: impl Fn(&T) -> bool,
) -> KindCounts {
    let runnable_total = results
        .iter()
        .filter(|r| kind_of(r) == Kind::Runnable)
        .count();
    let runnable_failed = results
        .iter()
        .filter(|r| kind_of(r) == Kind::Runnable && !passed_of(r))
        .count();
    KindCounts {
        runnable_total,
        runnable_passed: runnable_total - runnable_failed,
        runnable_failed,
        manual: results
            .iter()
            .filter(|r| kind_of(r) == Kind::Manual)
            .count(),
        empty: results.iter().filter(|r| kind_of(r) == Kind::Empty).count(),
    }
}

/// M124 (M106 ER-5): cap the drain buffer at 1 MiB. Pre-fix the buffer
/// grew unbounded under broad-scope verifications (e.g.
/// `cargo test --workspace` emitting several MB), risking OOM on
/// constrained CI runners (2GB GitHub Actions). Display layer still
/// truncates to 2000 chars so the dropped tail is never shown; the
/// sentinel preserves a "something was emitted and then dropped" signal.
pub const DRAIN_BUF_CAP_BYTES: usize = 1024 * 1024;

/// M106 (S1): drain a child process pipe on a background thread so the
/// verifier's main loop can poll `child.try_wait()` without blocking on
/// `read_to_end`. Without this, a child writing more than the kernel pipe
/// buffer (~64KB on macOS, similar on Linux) fills the pipe, blocks on
/// write, and the verifier's `try_wait` loop never sees child exit — the
/// deadlock that gated M104's completion.
///
/// **M124 (M106 ER-5):** the buffer is capped at [`DRAIN_BUF_CAP_BYTES`]
/// (1 MiB). Once the cap is hit, additional bytes are dropped and a
/// single `<output truncated at N bytes>` sentinel is appended so
/// consumers can detect "emitted-then-clipped" instead of mistaking it
/// for silent failure. The drain thread keeps reading from the pipe
/// regardless (so the child never blocks on a full kernel pipe buffer);
/// only the in-memory buffer is bounded.
///
/// Returns the thread's `JoinHandle`; the caller joins it after `child`
/// exits, then locks `buf` to read the captured bytes. Reading on a
/// dedicated thread drains the kernel pipe continuously, so the child
/// never blocks regardless of total output size. We push in 4 KB chunks
/// under a short-lived lock; the chunk size bounds the lock-hold time
/// (NOT the total buffer size — see [`DRAIN_BUF_CAP_BYTES`] for that).
pub fn pipe_drain_thread<R: std::io::Read + Send + 'static>(
    pipe: R,
    buf: Arc<Mutex<Vec<u8>>>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("ac_verify_pipe_drain".into())
        .spawn(move || {
            let mut pipe = pipe;
            let mut chunk = [0u8; 4096];
            let mut truncated = false;
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        let mut guard = buf.lock().expect("drain buf poisoned");
                        let prev_len = guard.len();
                        if prev_len >= DRAIN_BUF_CAP_BYTES {
                            // Already at cap. Append the truncation sentinel
                            // once (if not already appended) and continue
                            // dropping subsequent bytes. We must keep reading
                            // from `pipe` so the child doesn't block on a
                            // full kernel pipe buffer.
                            if !truncated {
                                let note =
                                    format!("<output truncated at {DRAIN_BUF_CAP_BYTES} bytes>");
                                guard.extend_from_slice(note.as_bytes());
                                truncated = true;
                            }
                            continue;
                        }
                        let remaining = DRAIN_BUF_CAP_BYTES - prev_len;
                        let to_copy = n.min(remaining);
                        guard.extend_from_slice(&chunk[..to_copy]);
                        if to_copy < n {
                            // Cap reached mid-chunk. Record sentinel once.
                            let note = format!("<output truncated at {DRAIN_BUF_CAP_BYTES} bytes>");
                            guard.extend_from_slice(note.as_bytes());
                            truncated = true;
                        }
                    }
                    // M109 (C-1): retry on EINTR (interrupted syscall); other
                    // errors (EIO, EPIPE, EAGAIN after genuine shutdown) end
                    // the loop but record the failure in the buffer's tail
                    // note via a sentinel byte so the verifier can surface
                    // a precise diagnostic instead of an opaque "no output".
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        let note = format!("<drain error: {e}>");
                        let mut guard = buf.lock().expect("drain buf poisoned");
                        guard.extend_from_slice(note.as_bytes());
                        break;
                    }
                }
            }
        })
        .expect("spawn drain thread")
}

/// M106 (S13/S14): poll `is_finished()` against `deadline`; only call
/// `join()` when the thread actually finished in time. On overflow,
/// detach via `mem::forget` so the verifier doesn't hang on a wedged
/// drain thread.
///
/// Without this, `JoinHandle::join()` blocks indefinitely once the
/// drain thread is wedged in `read()` on a pipe whose write end hasn't
/// fully closed — the underlying macOS read-ready / close race that
/// gated M104's completion.
///
/// **M107 (S4 / AC-03): accepted thread leak.** The overflow branch
/// below is the only path in `ac_verify` that detaches a thread via
/// `std::mem::forget` without joining it. The detached drain thread
/// will exit on its own when (a) the child subprocess is killed and
/// its stdout/stderr pipe write ends close at the kernel level (the
/// normal path), or (b) the orchestrator's process exits and the
/// thread is reaped implicitly. We trade a bounded thread-detach for
/// never blocking `ac_verify::execute` on a kernel that may not
/// deliver EOF promptly. This is a known trade-off, not a bug:
///
///   - In normal operation the drain thread exits in <2s after the
///     child is killed (its pipe write ends close).
///   - In the macOS read-ready/close race the thread exits when the
///     orchestrator process exits — at worst, mp exits cleanly with
///     no concurrency-bearing state.
///   - In neither case does the thread outlive the verifier
///     subprocess tree; the killpg-on-global-deadline path (M107 S3.2)
///     reap the child promptly, which closes the pipes.
///   - The thread consumes no shared state after the parent process
///     exits (only a small buffer and a kernel file descriptor).
///
/// Landed as the explicit "accepted leak" rather than fixed because
/// fixing it would require either (a) racing `wait` on a thread
/// that has no other interrupt signal, which re-introduces the
/// original hang, or (b) reaching into the drain thread's `read()`
/// to interrupt it, which is platform-specific and not generally
/// possible on stable Rust. Tracking in `mp-dogfood-log.md` for
/// future consideration.
fn bounded_join(handle: std::thread::JoinHandle<()>, deadline: Duration) {
    let deadline_at = std::time::Instant::now() + deadline;
    let mut finished = handle.is_finished();
    while !finished && std::time::Instant::now() < deadline_at {
        std::thread::sleep(Duration::from_millis(20));
        finished = handle.is_finished();
    }
    if finished {
        let _ = handle.join();
    } else {
        // Detach: thread continues to run until the pipe closes or
        // the process exits. We cannot block here without re-introducing
        // the original hang this helper is supposed to prevent.
        // See the function-level "accepted thread leak" note above.
        std::mem::forget(handle);
    }
}

/// M117 S1: send SIGKILL to the entire child process group on Unix.
/// The verifier is spawned with `Command::process_group(0)` (M107 S3),
/// so `pgid == child_pid` and `killpg(pgid, SIGKILL)` reaps the whole
/// tree (`sh → cargo → ...`) with one syscall. On non-Unix the call is
/// a no-op; the existing `child.kill()` after it still runs.
///
/// **Bounds check (M117 CR):** Linux PIDs are bounded by
/// `/proc/sys/kernel/pid_max` (default `4194304`, well under
/// `i32::MAX`). The previous version's `pid as i32` cast was
/// correct in practice (real-world PIDs are <1M) but wrapped
/// silently to a negative `i32` for any `u32` value larger than
/// `i32::MAX`, which would make `killpg` return `EINVAL` and the
/// orphan escape. We now `try_from` for a clear error path.
fn killpg_child(pid: u32) {
    #[cfg(unix)]
    {
        // SAFETY: `killpg(pgid, sig)` is a thin wrapper around the
        // POSIX killpg(2) syscall; it accepts a positive `pgid` and
        // translates internally to `kill(-pgid, sig)`. Passing the
        // positive `pid` here is correct (NOT `-pid`, which would
        // double-invert to a pid of `-pid` — see F-1 from the M107
        // external review, since removed; see git history for the
        // original bug report). ESRCH (no such process) is acceptable
        // because we are already in an error path; the desired post-
        // condition is "no orphan lives", not "killpg returned 0".
        if let Ok(pgid) = i32::try_from(pid) {
            unsafe {
                libc::killpg(pgid, libc::SIGKILL);
            }
        }
        // else: pid > i32::MAX is not a real-world Linux PID
        // (pid_max is ~4M, well under i32::MAX). The unwrap would be
        // safe but the explicit branch documents the cap.
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
    }
}

fn execute(
    mode: ExecutionMode,
    cwd: Option<&std::path::Path>,
    cancelled: &Arc<AtomicBool>,
    child_pids: &Arc<Mutex<Vec<u32>>>,
) -> Result<(i32, String), String> {
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    let mut command = match mode {
        ExecutionMode::Argv(argv) => {
            let mut command = Command::new(&argv[0]);
            command.args(&argv[1..]);
            command
        }
        ExecutionMode::Shell(command_text) => {
            let mut command = Command::new("sh");
            command.arg("-c").arg(command_text);
            command
        }
    };
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    // M107 (S3): place the child in its own process group so the
    // orchestrator's global-deadline handler can `killpg(pgid, SIGKILL)`
    // and take the whole subtree (sh → cargo → rsrc) with one syscall.
    // `process_group(0)` makes the child a process-group leader with
    // pgid == child_pid; `killpg` on positive pgid kills that group.
    #[cfg(unix)]
    let command = command.process_group(0);

    let mut child = command.spawn().map_err(|e| e.to_string())?;

    // M117 S1: the cancel/timeout paths below call this helper instead
    // of `child.kill()` so the entire child process group (and any
    // subprocesses the verifier forked) is reaped. The child is its
    // own process-group leader (`pgid == child_pid`) thanks to the
    // `Command::process_group(0)` setup above, so killpg on the
    // positive pid is one syscall. See B-52 / ER-5 for the original
    // reviewer finding.
    let child_pid = child.id();

    // M107 (S3): register the child's pid (== pgid, see process_group
    // call above) into the orchestrator's kill-list so the global-
    // deadline handler can clean it up even if the cooperative flag
    // is ignored (e.g., the verifier is wedged in an FFI call before
    // reaching its next cancel-check). The pid is recorded even on
    // success paths — the orchestrator only consults the list on
    // timeout, and an empty registry is a no-op.
    if let Ok(mut reg) = child_pids.lock() {
        // `Child::id()` returns `u32` on stable Rust ≥ 1.74 (the
        // method was unified with `id()` after the `Option<u32>`
        // API was deprecated). We register the value verbatim
        // for the kill-on-timeout path before any async work
        // happens. Lock poisoning means someone panicked; a
        // missing pid in the kill-set is acceptable on the error
        // path (the killpg loop just iterates
        // one fewer entry).
        reg.push(child.id());
    }

    // M106 (S2): drain pipes on background threads (added in S1). The
    // previous version read stdout/stderr only at child exit, which
    // deadlocked whenever the child emitted more than the kernel pipe
    // buffer (~64KB) — the very bug that gated M104. The reader threads
    // consume continuously, so the kernel pipe never fills.
    let stdout_buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let stderr_buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let stdout_handle = child
        .stdout
        .take()
        .map(|p| pipe_drain_thread(p, Arc::clone(&stdout_buf)));
    let stderr_handle = child
        .stderr
        .take()
        .map(|p| pipe_drain_thread(p, Arc::clone(&stderr_buf)));

    let timeout = verify_timeout_dur();
    let start = Instant::now();

    let status = loop {
        // M107 (S3): observe the cooperative cancel flag between polls.
        // Cheap relaxed load; the ordering is irrelevant because the
        // orchestrator's flip is a hint, not a synchronization barrier.
        if cancelled.load(Ordering::Relaxed) {
            // M117 S1: kill the entire process group (pgid == child_pid
            // since the child is its own group leader per M107 S3) so
            // any subprocesses the verifier forked (cargo build, rsrc,
            // etc.) die with one syscall. Single-pid `child.kill()` only
            // reaches the leaf; M107's review (B-52 / ER-5) flagged the
            // equivalent per-AC gap.
            killpg_child(child_pid);
            let _ = child.kill();
            let _ = child.wait();
            const DRAIN_JOIN_TIMEOUT: Duration = Duration::from_secs(2);
            if let Some(h) = stdout_handle {
                bounded_join(h, DRAIN_JOIN_TIMEOUT);
            }
            if let Some(h) = stderr_handle {
                bounded_join(h, DRAIN_JOIN_TIMEOUT);
            }
            return Err("cancelled by global-deadline".to_string());
        }
        // M109 (C-1): retry on EINTR (which is normal on a signal-
        // interrupted syscall), surface any other try_wait error with
        // its ErrorKind so the orchestrator / VerifyReport can diagnose
        // ECHILD (already reaped by a parent), ESRCH (no such process),
        // EPERM, etc. distinctly from the per-AC timeout.
        let try_wait_done = match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => false,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => false,
            Err(e) => {
                return Err(format!("try_wait failed: kind={:?} error={}", e.kind(), e));
            }
        };
        if !try_wait_done && start.elapsed() >= timeout {
            // M117 S1: killpg on the per-AC timeout path (same shape as
            // the cooperative-cancel path above — `pgid == child_pid`
            // because the child was started with `process_group(0)`).
            // Closes the B-52 / ER-5 reviewer finding that the per-AC
            // path used `child.kill()` (single-pid SIGKILL) which leaves
            // forked subprocesses orphaned when they didn't propagate
            // signals.
            killpg_child(child_pid);
            let _ = child.kill();
            let _ = child.wait();
            // M106 (S13): bounded join on the drain threads. The
            // thread can wedge in `read()` on macOS even after the
            // child has been killed; see `bounded_join` for the
            // poll-then-detach dance that avoids hanging.
            const DRAIN_JOIN_TIMEOUT: Duration = Duration::from_secs(2);
            if let Some(h) = stdout_handle {
                bounded_join(h, DRAIN_JOIN_TIMEOUT);
            }
            if let Some(h) = stderr_handle {
                bounded_join(h, DRAIN_JOIN_TIMEOUT);
            }
            return Err(format!(
                "verification timed out after {}s",
                timeout.as_secs()
            ));
        }
        if !try_wait_done {
            thread::sleep(Duration::from_millis(50));
        }
    };

    // M106 (S14): same bounded-join treatment as the timeout path.
    // On the success path the child has exited, so the drain threads
    // usually finish promptly — but on macOS or under load the kernel
    // pipe close-and-read-ready race can still leave a reader wedged
    // in `read()`. Bound the join to 2s; on overflow, drop the handle
    // (thread detached, runs to natural exit when its pipe closes).
    // Captured bytes may be partial in the pathological case, but
    // the bounded join keeps the verifier responsive.
    //
    // Lock the buffer first (cheap, unordered) so it's captured even
    // when we have to detach the handle. The lock happens before the
    // bounded join call below.
    //
    // M107 (S8) budget alignment: total per-AC verifier wall-clock is
    // bounded by `MP_VERIFY_TIMEOUT_SECS` (default 300s, configurable)
    // + `SUCCESS_DRAIN_JOIN_TIMEOUT = 2s` per drain thread (×2 for
    // stdout/stderr) + bounded-join slack + scheduler overhead. The
    // regression test for the success-path bounded join is in
    // `crates/mp/tests/ac_verify_drain_join_timeout.rs` and asserts
    // wall-clock `elapsed < 8s` for `MP_VERIFY_TIMEOUT_SECS=2 + sleep 30`.
    // Budget breakdown: 2s per-AC + 2s bounded-join (stdout) + 2s bounded-
    // join (stderr) ≈ 6s worst-case; the 8s assertion has ~2s of headroom
    // for scheduler slop on slow CI. If you change
    // `SUCCESS_DRAIN_JOIN_TIMEOUT` below, update the test's
    // `assertion.elapsed < Duration::from_secs(8)` line to match
    // `(per-AC timeout) + 2 × SUCCESS_DRAIN_JOIN_TIMEOUT + slack`.
    const SUCCESS_DRAIN_JOIN_TIMEOUT: Duration = Duration::from_secs(2);
    let capture_bytes =
        |handle: Option<std::thread::JoinHandle<()>>, buf: &Arc<Mutex<Vec<u8>>>| -> Vec<u8> {
            let bytes = buf.lock().expect("drain buf poisoned").clone();
            if let Some(h) = handle {
                bounded_join(h, SUCCESS_DRAIN_JOIN_TIMEOUT);
            }
            bytes
        };
    let stdout_bytes: Vec<u8> = capture_bytes(stdout_handle, &stdout_buf);
    let stderr_bytes: Vec<u8> = capture_bytes(stderr_handle, &stderr_buf);

    let code = status.code().unwrap_or(-1);
    let mut combined = String::from_utf8_lossy(&stdout_bytes).to_string();
    if !stderr_bytes.is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(&String::from_utf8_lossy(&stderr_bytes));
    }
    Ok((code, combined))
}

const DEFAULT_VERIFY_TIMEOUT_SECS: u64 = 300;

#[allow(dead_code)]
fn verify_timeout_secs() -> u64 {
    verify_timeout_dur().as_secs()
}

/// M107 (S3): returns the per-AC verifier timeout as a `Duration`.
/// New entry point preferred over `verify_timeout_secs()` because tests
/// can inject a custom deadline via this path without mutating the
/// process-global `MP_VERIFY_TIMEOUT_SECS` environment variable (which
/// would race parallel tests). The env-var override remains in place
/// for ops use.
fn verify_timeout_dur() -> Duration {
    let thread_override = THREAD_TIMEOUT_OVERRIDE.with(|c| *c.borrow());
    verify_timeout_dur_override(thread_override)
}

/// M117 S1: same as `verify_timeout_dur` but accepts an explicit
/// override. The default `None` reads `MP_VERIFY_TIMEOUT_SECS` from
/// the environment; `Some(n)` bypasses that lookup, which is useful
/// in tests where setting an env var is racy under `cargo test`'s
/// parallel harness.
fn verify_timeout_dur_override(override_secs: Option<u64>) -> Duration {
    override_secs
        .map(Duration::from_secs)
        .or_else(|| {
            std::env::var("MP_VERIFY_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs)
        })
        .unwrap_or(Duration::from_secs(DEFAULT_VERIFY_TIMEOUT_SECS))
}

// M117 S1: thread-local override for the per-AC verifier timeout.
// Used by tests in `crates/mp/tests/ac_verify_per_ac_timeout_killpg.rs`
// to inject a deterministic deadline that completes inside the
// test's wall-clock budget (default `MP_VERIFY_TIMEOUT_SECS = 300s`
// would make the regression take 5+ minutes per test run).
// Production code never sets this; the override slot is `None` and
// `verify_timeout_dur` falls through to the env var / default.
thread_local! {
    static THREAD_TIMEOUT_OVERRIDE: std::cell::RefCell<Option<u64>> = const { std::cell::RefCell::new(None) };
}

/// Test-only scoped setter for `THREAD_TIMEOUT_OVERRIDE`. Sets the
/// override for the duration of `f`, then restores the prior value
/// (whether that was `None` or `Some(prior_secs)`). Production code
/// does not invoke this function.
///
/// **Why scoped (M117 CR):** the prior raw setter `__set_thread_timeout_override_for_test`
/// was sticky: it left the override set on the calling thread forever.
/// A test that forgot the `std::thread::spawn` discipline would leak
/// the override into sibling tests running on the same thread. The
/// scoped closure makes the lifetime of the override lexically
/// visible at the call site — `Drop` runs in reverse order on the
/// guard's stack frame, restoring the prior value before the caller
/// resumes.
#[doc(hidden)]
pub fn with_thread_timeout_override<R>(secs: u64, f: impl FnOnce() -> R) -> R {
    struct OverrideGuard(Option<u64>);
    impl Drop for OverrideGuard {
        fn drop(&mut self) {
            THREAD_TIMEOUT_OVERRIDE.with(|c| *c.borrow_mut() = self.0);
        }
    }
    let prior = THREAD_TIMEOUT_OVERRIDE.with(|c| *c.borrow());
    THREAD_TIMEOUT_OVERRIDE.with(|c| *c.borrow_mut() = Some(secs));
    let _guard = OverrideGuard(prior);
    f()
}

fn truncate(s: String, max: usize) -> String {
    if s.chars().count() <= max {
        return s;
    }
    let mut t: String = s.chars().take(max.saturating_sub(3)).collect();
    t.push_str("...");
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn pipe_drain_thread_drains_small_input() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let handle = pipe_drain_thread(Cursor::new(b"hello".to_vec()), Arc::clone(&buf));
        handle.join().expect("drain thread join");
        let captured = buf.lock().expect("drain buf poisoned").clone();
        assert_eq!(captured, b"hello");
    }

    #[test]
    fn pipe_drain_thread_drains_large_input() {
        // >64KB of input — well above any reasonable kernel pipe buffer.
        let payload: Vec<u8> = (0..200_000u32).map(|i| (i & 0xFF) as u8).collect();
        let expected_len = payload.len();
        let buf = Arc::new(Mutex::new(Vec::new()));
        let handle = pipe_drain_thread(Cursor::new(payload), Arc::clone(&buf));
        handle.join().expect("drain thread join");
        let captured = buf.lock().expect("drain buf poisoned").clone();
        assert_eq!(captured.len(), expected_len, "all bytes drained");
        // Confirm content fidelity (loop pattern repeats every 256 bytes).
        assert_eq!(captured[0], 0x00);
        assert_eq!(captured[256], 0x00);
        assert_eq!(captured[257], 0x01);
    }

    #[test]
    fn pipe_drain_thread_handles_eof_immediately() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let handle = pipe_drain_thread(Cursor::new(Vec::<u8>::new()), Arc::clone(&buf));
        handle.join().expect("drain thread join");
        assert!(buf.lock().expect("drain buf poisoned").is_empty());
    }

    #[test]
    fn pipe_drain_thread_records_error_kind_on_non_eintr() {
        // M109 (C-1): a non-EINTR read error should now surface as a
        // sentinel `<drain error: ...>` suffix in the captured buffer so
        // the verifier can distinguish drain errors from clean EOF.
        // Pre-C-1, the loop would `break` silently and truncate the
        // captured output. Post-C-1, the sentinel appears.
        struct ReadThenOtherError;
        impl std::io::Read for ReadThenOtherError {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if buf.is_empty() {
                    return Ok(0);
                }
                buf[0] = b'X';
                // Return a non-Interrupted, non-Other default error
                // so the post-C-1 branch fires.
                Err(std::io::Error::other("synthetic drain error"))
            }
        }
        let buf = Arc::new(Mutex::new(Vec::new()));
        let read = ReadThenOtherError;
        let handle = pipe_drain_thread(read, Arc::clone(&buf));
        handle.join().expect("drain thread join");
        let captured = buf.lock().expect("drain buf poisoned").clone();
        let s = String::from_utf8_lossy(&captured);
        assert!(
            s.contains("<drain error"),
            "post-C-1 pipe_drain_thread should record the error kind; got: {s:?}"
        );
    }

    #[test]
    fn pipe_drain_thread_retries_on_interrupted_error() {
        // M109 (C-1): an EINTR (interrupted syscall) error from read
        // should be RETRIED rather than terminating the loop. We use a
        // Read stub that returns one Interrupted then the rest of the
        // payload, and confirm the captured output is the full payload
        // (i.e., the Interrupted error did not terminate the loop).
        struct InterThenEof {
            first: bool,
            tail: Vec<u8>,
        }
        impl std::io::Read for InterThenEof {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.first {
                    self.first = false;
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "synthetic EINTR",
                    ));
                }
                if self.tail.is_empty() {
                    return Ok(0); // clean EOF
                }
                let n = self.tail.len().min(buf.len());
                buf[..n].copy_from_slice(&self.tail[..n]);
                self.tail.drain(..n);
                Ok(n)
            }
        }
        let buf = Arc::new(Mutex::new(Vec::new()));
        let read = InterThenEof {
            first: true,
            tail: b"hello world".to_vec(),
        };
        let handle = pipe_drain_thread(read, Arc::clone(&buf));
        handle.join().expect("drain thread join");
        let captured = buf.lock().expect("drain buf poisoned").clone();
        // Post-C-1: the Interrupted error must NOT terminate the loop;
        // the rest of the payload should be captured.
        assert_eq!(
            captured,
            b"hello world",
            "EINTR must be retried, not break the drain loop; got: {:?}",
            String::from_utf8_lossy(&captured)
        );
    }

    fn sample_ac(verification: &str) -> AcceptanceCriterion {
        AcceptanceCriterion {
            id: "AC-01".into(),
            description: "fixture".into(),
            verification: verification.into(),
            status: "pending".into(),
            evidence: String::new(),
        }
    }

    #[test]
    fn command_cache_skips_second_identical_success() {
        let mut cache = CommandCache::new();
        let ac = sample_ac("test 0 -eq 0");
        let cancelled = never_cancelled();
        let child_pids = empty_child_pids();
        let r1 = run_one_in(&ac, None, &cancelled, &child_pids, Some(&mut cache));
        let r2 = run_one_in(&ac, None, &cancelled, &child_pids, Some(&mut cache));
        assert!(r1.passed);
        assert!(r2.passed);
        assert!(r1.note.contains("succeeded"));
        assert!(r2.note.contains("cache hit"));
    }

    #[test]
    fn command_cache_skips_second_identical_failure() {
        let mut cache = CommandCache::new();
        let ac = sample_ac("test 1 -eq 2");
        let cancelled = never_cancelled();
        let child_pids = empty_child_pids();
        let r1 = run_one_in(&ac, None, &cancelled, &child_pids, Some(&mut cache));
        let r2 = run_one_in(&ac, None, &cancelled, &child_pids, Some(&mut cache));
        assert!(!r1.passed);
        assert!(!r2.passed);
        assert!(r2.note.contains("cache hit"));
    }

    #[test]
    fn command_cache_not_used_when_disabled() {
        let ac = sample_ac("test 0 -eq 0");
        let cancelled = never_cancelled();
        let child_pids = empty_child_pids();
        let r1 = run_one_in(&ac, None, &cancelled, &child_pids, None);
        let r2 = run_one_in(&ac, None, &cancelled, &child_pids, None);
        assert!(r1.passed);
        assert!(r2.passed);
        assert!(!r2.note.contains("cache hit"));
    }

    #[test]
    fn legacy_cargo_test_name_rewrites_to_suite_binary() {
        let cmd = "cargo test -p mp --test milestone_complete_gate_caching";
        let out = command_for_execution(cmd);
        assert!(out.contains("--test suite_milestone milestone_complete_gate_caching::"));
        assert!(!out.contains("--test milestone_complete_gate_caching"));
    }

    /// mp-oracle still ships a real `mini_schema_parity` binary; the mp-only
    /// legacy map must not rewrite other packages.
    #[test]
    fn legacy_rewrite_skips_non_mp_package_mini_schema_parity() {
        let cmd = "cargo nextest run -p mp-oracle --test mini_schema_parity --no-fail-fast";
        let out = command_for_execution(cmd);
        assert_eq!(out, cmd);
        assert!(!out.contains("suite_validate"));
    }

    #[test]
    fn legacy_rewrite_still_maps_mp_mini_schema_parity() {
        let cmd = "cargo nextest run -p mp --test mini_schema_parity --no-fail-fast";
        let out = command_for_execution(cmd);
        assert!(
            out.contains("--test suite_validate mini_schema_parity::"),
            "expected mp legacy rewrite, got: {out}"
        );
        assert!(!out.contains("--test mini_schema_parity "));
    }

    #[test]
    fn legacy_rewrite_package_scoped_in_compound_command() {
        let cmd = "cargo test -p mp --test mini_schema_parity && cargo nextest run -p mp-oracle --test mini_schema_parity --no-fail-fast";
        let out = command_for_execution(cmd);
        assert!(
            out.contains("-p mp --test suite_validate mini_schema_parity::"),
            "mp side must rewrite: {out}"
        );
        assert!(
            out.contains("-p mp-oracle --test mini_schema_parity --no-fail-fast"),
            "mp-oracle side must stay unchanged: {out}"
        );
        assert!(
            !out.contains("-p mp-oracle --test suite_validate"),
            "must not contaminate mp-oracle: {out}"
        );
    }

    #[test]
    fn legacy_rewrite_honors_package_after_test_flag() {
        let cmd = "cargo test --test milestone_complete_gate_caching -p mp";
        let out = command_for_execution(cmd);
        assert!(
            out.contains("--test suite_milestone milestone_complete_gate_caching::"),
            "expected rewrite when -p mp follows --test: {out}"
        );
    }

    #[test]
    fn legacy_rewrite_skips_unscoped_test_flag() {
        // Without an explicit -p mp, do not apply the mp consolidation map.
        let cmd = "cargo test --test mini_schema_parity";
        let out = command_for_execution(cmd);
        assert_eq!(out, cmd);
    }

    #[test]
    fn bare_rs_path_rewrites_to_suite_binary() {
        let cmd = command_for_execution("crates/mp/tests/verify_lint_portability.rs");
        assert!(cmd.contains("--test suite_validate verify_lint_portability::"));
    }

    #[test]
    fn bare_rs_path_rejects_shell_metacharacters() {
        assert!(translate_bare_rs_test_path("crates/$(id)/tests/foo.rs").is_none());
        assert!(translate_bare_rs_test_path("crates/`id`/tests/foo.rs").is_none());
        assert!(translate_bare_rs_test_path("crates/mp/tests/foo;rm.rs").is_none());
        assert!(translate_bare_rs_test_path("crates/../etc/tests/foo.rs").is_none());
        assert!(translate_bare_rs_test_path("crates/mp/tests/foo.rs").is_some());
    }

    #[test]
    fn classify_respects_verify_no_shell() {
        // Inject the flag — do not mutate process env (races parallel tests
        // that call classify / run_one concurrently).
        assert_eq!(classify_with("cargo test -p mp", true), Kind::Manual);
        assert_eq!(classify_with("cargo test -p mp", false), Kind::Runnable);
        assert_eq!(classify_with("manual: eyeball it", false), Kind::Manual);
        assert_eq!(classify_with("manual: eyeball it", true), Kind::Manual);
    }

    #[test]
    fn classify_with_prose_parenthetical_note_is_manual() {
        assert_eq!(
            classify_with(
                "crates/raul/tests/tui_view_state.rs (grep-based test)",
                false
            ),
            Kind::Manual
        );
        assert_eq!(
            classify_with(
                "crates/raul/tests/keybinds.rs (load from JSON then assert default on missing entries)",
                false
            ),
            Kind::Manual
        );
    }

    #[test]
    fn classify_with_prose_plus_rg_clause_is_manual() {
        assert_eq!(
            classify_with(
                "crates/raul/tests/keybinds.rs + rg for hardcoded key legends in crates/raul/src/tui/render/",
                false
            ),
            Kind::Manual
        );
        assert_eq!(
            classify_with("src/foo.rs + grep for TODO markers", false),
            Kind::Manual
        );
        assert_eq!(
            classify_with("notes + find leftover fixtures", false),
            Kind::Manual
        );
    }

    #[test]
    fn classify_with_prose_semicolon_clauses_is_manual() {
        assert_eq!(
            classify_with(
                "cargo nextest run -p mp --no-fail-fast ; all green ; ≥10 unit tests covering positive cases",
                false
            ),
            Kind::Manual
        );
    }

    #[test]
    fn classify_with_prose_multi_and_conjunctions_is_manual() {
        assert_eq!(
            classify_with(
                "confirm the gate runs and the cache hits and the note is set",
                false
            ),
            Kind::Manual
        );
    }

    #[test]
    fn prose_detector_legitimate_subshell_stays_runnable() {
        assert_eq!(
            classify_with("(cd dir && cargo test -p mp)", false),
            Kind::Runnable
        );
        assert_eq!(
            classify_with("echo $(cargo metadata --format-version 1)", false),
            Kind::Runnable
        );
    }

    #[test]
    fn prose_detector_legitimate_commands_stay_runnable() {
        assert_eq!(classify_with("cargo test -p mp", false), Kind::Runnable);
        assert_eq!(classify_with("make test", false), Kind::Runnable);
        assert_eq!(classify_with("mp validate", false), Kind::Runnable);
        assert_eq!(
            classify_with("crates/mp/tests/workflow_gates.rs", false),
            Kind::Runnable
        );
        assert_eq!(
            classify_with("cd crates/mp; cargo test -p mp", false),
            Kind::Runnable
        );
        assert_eq!(classify_with("rg something", false), Kind::Runnable);
        assert_eq!(
            classify_with("./scripts/audit-step-tests.sh", false),
            Kind::Runnable
        );
    }

    /// M177 external F-07: parens inside quotes are not prose notes.
    /// Repo-canonical nextest filters (`-E 'test(/name/)'`) and quoted
    /// shell strings must stay Runnable even when the quoted body has
    /// spaces or hyphens.
    #[test]
    fn prose_detector_quoted_parens_and_nextest_filters_stay_runnable() {
        assert_eq!(
            classify_with(
                "cargo nextest run -p mp -E 'test(/foo-bar/)' --no-fail-fast",
                false
            ),
            Kind::Runnable
        );
        assert_eq!(
            classify_with(
                "cargo nextest run -p mp -E 'test(/ac_update_emits_prose_warn|verification_warns/)' --no-fail-fast",
                false
            ),
            Kind::Runnable
        );
        assert_eq!(
            classify_with("echo 'hello (world with spaces)'", false),
            Kind::Runnable
        );
        assert_eq!(
            classify_with(r#"echo "note (pre-m154)""#, false),
            Kind::Runnable
        );
        // Unquoted prose parenthetical still Manual.
        assert_eq!(
            classify_with(
                "crates/raul/tests/tui_view_state.rs (grep-based test)",
                false
            ),
            Kind::Manual
        );
    }

    #[test]
    fn prose_detector_manual_prefix_still_wins() {
        assert_eq!(
            classify_with(
                "manual: crates/raul/tests/tui_view_state.rs (grep-based test)",
                false
            ),
            Kind::Manual
        );
        assert!(looks_like_prose(
            "crates/raul/tests/tui_view_state.rs (grep-based test)"
        ));
        assert!(!looks_like_prose("cargo test -p mp"));
        assert!(!looks_like_prose("(cd dir && make test)"));
    }

    #[test]
    fn prose_detector_plus_awk_sed_find() {
        assert!(looks_like_prose("output.log + awk '{print $1}'"));
        assert!(looks_like_prose("notes.md + sed -n '1,5p'"));
        assert!(looks_like_prose("tree + find orphan files"));
    }

    #[test]
    fn prose_detector_shell_for_loop_stays_runnable() {
        assert_eq!(
            classify_with(
                "for i in $(seq 10); do cargo test -p raul >/dev/null 2>&1 || exit 1; done",
                false
            ),
            Kind::Runnable
        );
    }

    #[test]
    fn prose_detector_quoted_semicolon_in_awk_stays_runnable() {
        // M159-style: `; ` inside single quotes must not force Manual.
        let v = "awk 'BEGIN{f=0} /pattern/{f=1; next} f{print; exit}' Cargo.toml | grep -q ok";
        assert_eq!(classify_with(v, false), Kind::Runnable);
        assert!(!looks_like_prose(v));
    }

    #[test]
    fn prose_detector_export_and_bang_prefix_stay_runnable() {
        assert_eq!(
            classify_with("export RUST_BACKTRACE=1; cargo nextest run -p mp", false),
            Kind::Runnable
        );
        assert_eq!(
            classify_with("cargo test; ! grep -q TODO src/", false),
            Kind::Runnable
        );
        assert_eq!(
            classify_with("FOO=1 cargo test -p mp", false),
            Kind::Runnable
        );
    }
}
