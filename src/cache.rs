//! On-disk cache that makes re-runs fast. Everything lives under one
//! tool-specific directory in the XDG cache location
//! (`$XDG_CACHE_HOME/contributor-graphs`, else `~/.cache/contributor-graphs`):
//!
//! - `clones/<key>/`     bare partial clones (see [`crate::repo`]).
//! - `commits/<key>.json` parsed `git log` output, keyed by the branch tip SHA
//!   so it is reused only while the history is unchanged.
//! - `github/authors.json`  commit SHA -> resolved GitHub login + avatar URL
//!   (immutable, so never expires).
//! - `github/profiles.json` login -> display name + company (time-limited).
//! - `github/avatars.json`  avatar URL -> embedded data URI (time-limited).
//!
//! The git-history and clone caches alone turn a multi-minute whole-org run
//! into a few seconds when nothing has changed; the GitHub caches remove the
//! thousands of API calls and avatar downloads on top of that.

use crate::model::CommitFilter;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Bump when the cached layout changes in a backwards-incompatible way.
const COMMITS_VERSION: u32 = 1;
/// Profiles can change (a company move), so they expire.
const PROFILE_TTL: i64 = 30 * 24 * 60 * 60;
/// Avatars change rarely; keep them longer.
const AVATAR_TTL: i64 = 90 * 24 * 60 * 60;
/// An org's repo list changes as repos are added or archived; keep it briefly
/// so back-to-back runs are fast but new repos still appear the same day.
const ORG_TTL: i64 = 6 * 60 * 60;

/// The tool's cache directory, or `None` if no home/XDG dir can be found.
pub fn root() -> Option<PathBuf> {
    if let Ok(x) = std::env::var("XDG_CACHE_HOME") {
        let x = x.trim();
        if !x.is_empty() {
            return Some(PathBuf::from(x).join("contributor-graphs"));
        }
    }
    let home = std::env::var("HOME").ok().filter(|h| !h.is_empty())?;
    Some(
        PathBuf::from(home)
            .join(".cache")
            .join("contributor-graphs"),
    )
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn write_json<T: Serialize>(path: &Path, value: &T) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string(value) {
        let _ = std::fs::write(path, s);
    }
}

/// A commit as stored in the git-history cache (no source index; that is
/// re-assigned per run when the pool is rebuilt).
#[derive(Clone, Serialize, Deserialize)]
pub struct CachedCommit {
    pub sha: String,
    pub ts: i64,
    pub name: String,
    pub email: String,
}

/// One repository's cached `git log`, valid only while `tip` and the filters
/// still match the live repository.
#[derive(Serialize, Deserialize)]
pub struct CommitsCache {
    pub version: u32,
    pub tip: String,
    pub since: Option<String>,
    pub until: Option<String>,
    pub no_merges: bool,
    pub commits: Vec<CachedCommit>,
}

fn commits_path(root: &Path, key: &str) -> PathBuf {
    root.join("commits").join(format!("{key}.json"))
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Author {
    pub login: String,
    pub avatar_url: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct ProfileRec {
    name: Option<String>,
    company: Option<String>,
    ts: i64,
}

#[derive(Clone, Serialize, Deserialize)]
struct AvatarRec {
    data: String,
    ts: i64,
}

#[derive(Clone, Serialize, Deserialize)]
struct OrgRec {
    repos: Vec<String>,
    ts: i64,
}

/// In-memory view of the GitHub caches for one run. Loaded once, queried before
/// any network call, and written back at the end. Reads are skipped (treated as
/// misses) when `refresh` is set, but existing entries are still preserved on
/// save so unrelated repositories keep their cache.
pub struct Caches {
    root: Option<PathBuf>,
    refresh: bool,
    now: i64,
    authors: HashMap<String, Author>,
    profiles: HashMap<String, ProfileRec>,
    avatars: HashMap<String, AvatarRec>,
    orgs: HashMap<String, OrgRec>,
    authors_dirty: bool,
    profiles_dirty: bool,
    avatars_dirty: bool,
    orgs_dirty: bool,
}

impl Caches {
    pub fn load(refresh: bool, now: i64) -> Self {
        let root = root();
        let dir = root.as_ref().map(|r| r.join("github"));
        fn load_map<V: for<'de> Deserialize<'de>>(
            dir: &Option<PathBuf>,
            name: &str,
        ) -> HashMap<String, V> {
            dir.as_ref()
                .and_then(|d| read_json(&d.join(name)))
                .unwrap_or_default()
        }
        Caches {
            authors: load_map(&dir, "authors.json"),
            profiles: load_map(&dir, "profiles.json"),
            avatars: load_map(&dir, "avatars.json"),
            orgs: load_map(&dir, "orgs.json"),
            root,
            refresh,
            now,
            authors_dirty: false,
            profiles_dirty: false,
            avatars_dirty: false,
            orgs_dirty: false,
        }
    }

    /// Cached commits for a repo, returned only if the freshness token (`tip`)
    /// and the filters still match this run.
    pub fn commits(
        &self,
        key: &str,
        tip: &str,
        filter: &CommitFilter,
    ) -> Option<Vec<CachedCommit>> {
        if self.refresh {
            return None;
        }
        let entry: CommitsCache = read_json(&commits_path(self.root.as_ref()?, key))?;
        (entry.version == COMMITS_VERSION
            && entry.tip == tip
            && entry.since == filter.since
            && entry.until == filter.until
            && entry.no_merges == filter.no_merges)
            .then_some(entry.commits)
    }

    /// Store a repo's parsed commits against the current tip SHA.
    pub fn put_commits(
        &self,
        key: &str,
        tip: &str,
        filter: &CommitFilter,
        commits: Vec<CachedCommit>,
    ) {
        let Some(root) = &self.root else { return };
        let entry = CommitsCache {
            version: COMMITS_VERSION,
            tip: tip.to_string(),
            since: filter.since.clone(),
            until: filter.until.clone(),
            no_merges: filter.no_merges,
            commits,
        };
        write_json(&commits_path(root, key), &entry);
    }

    pub fn author(&self, sha: &str) -> Option<Author> {
        if self.refresh {
            return None;
        }
        self.authors.get(sha).cloned()
    }

    pub fn put_author(&mut self, sha: String, login: String, avatar_url: String) {
        self.authors.insert(sha, Author { login, avatar_url });
        self.authors_dirty = true;
    }

    pub fn profile(&self, login: &str) -> Option<(Option<String>, Option<String>)> {
        if self.refresh {
            return None;
        }
        let rec = self.profiles.get(login)?;
        (self.now - rec.ts < PROFILE_TTL).then(|| (rec.name.clone(), rec.company.clone()))
    }

    pub fn put_profile(&mut self, login: String, name: Option<String>, company: Option<String>) {
        self.profiles.insert(
            login,
            ProfileRec {
                name,
                company,
                ts: self.now,
            },
        );
        self.profiles_dirty = true;
    }

    pub fn avatar(&self, key: &str) -> Option<String> {
        if self.refresh {
            return None;
        }
        let rec = self.avatars.get(key)?;
        (self.now - rec.ts < AVATAR_TTL).then(|| rec.data.clone())
    }

    pub fn put_avatar(&mut self, key: String, data: String) {
        self.avatars.insert(key, AvatarRec { data, ts: self.now });
        self.avatars_dirty = true;
    }

    /// The cached repo list for an org/user, if still within its short TTL.
    pub fn org_repos(&self, owner: &str) -> Option<Vec<String>> {
        if self.refresh {
            return None;
        }
        let rec = self.orgs.get(owner)?;
        (self.now - rec.ts < ORG_TTL).then(|| rec.repos.clone())
    }

    pub fn put_org_repos(&mut self, owner: String, repos: Vec<String>) {
        self.orgs.insert(
            owner,
            OrgRec {
                repos,
                ts: self.now,
            },
        );
        self.orgs_dirty = true;
    }

    /// Persist any maps that changed this run.
    pub fn save(&self) {
        let Some(root) = &self.root else { return };
        let dir = root.join("github");
        if self.authors_dirty {
            write_json(&dir.join("authors.json"), &self.authors);
        }
        if self.profiles_dirty {
            write_json(&dir.join("profiles.json"), &self.profiles);
        }
        if self.avatars_dirty {
            write_json(&dir.join("avatars.json"), &self.avatars);
        }
        if self.orgs_dirty {
            write_json(&dir.join("orgs.json"), &self.orgs);
        }
    }
}
