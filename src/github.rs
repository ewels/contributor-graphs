use crate::cache::Caches;
use crate::identity::Cluster;
use crate::model::{Commit, Contributor};
use base64::Engine;
use std::collections::HashMap;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

const THREADS: usize = 8;

/// A GitHub user profile reduced to (display name, affiliation).
type Profile = (Option<String>, Option<String>);

pub struct GhClient {
    agent: ureq::Agent,
    token: Option<String>,
}

/// Get a GitHub token: $GITHUB_TOKEN / $GH_TOKEN, else `gh auth token`.
pub fn find_token() -> Option<String> {
    for var in ["GITHUB_TOKEN", "GH_TOKEN"] {
        if let Ok(t) = std::env::var(var) {
            let t = t.trim().to_string();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    let out = Command::new("gh").args(["auth", "token"]).output().ok()?;
    if out.status.success() {
        let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !t.is_empty() {
            return Some(t);
        }
    }
    None
}

/// Parse GitHub noreply addresses: `12345+login@users.noreply.github.com`
/// or the older `login@users.noreply.github.com`.
pub fn parse_noreply(email: &str) -> Option<(Option<u64>, String)> {
    let local = email.strip_suffix("@users.noreply.github.com")?;
    match local.split_once('+') {
        Some((id, login)) => {
            let id = id.parse::<u64>().ok();
            Some((id, login.to_string()))
        }
        None => Some((None, local.to_string())),
    }
}

impl GhClient {
    pub fn new(token: Option<String>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(30))
            .user_agent("contributor-graphs (https://github.com/ewels/contributor-graphs)")
            .build();
        GhClient { agent, token }
    }

    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    fn get_json(&self, url: &str) -> Option<serde_json::Value> {
        let mut req = self
            .agent
            .get(url)
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", "2022-11-28");
        if let Some(t) = &self.token {
            req = req.set("Authorization", &format!("Bearer {t}"));
        }
        match req.call() {
            Ok(resp) => resp.into_json().ok(),
            Err(ureq::Error::Status(code, _)) => {
                if code == 403 || code == 429 {
                    eprintln!("  warning: GitHub API rate limited (HTTP {code})");
                }
                None
            }
            Err(_) => None,
        }
    }

    /// Resolve the GitHub login + avatar for the author of a commit.
    fn commit_author(&self, slug: &str, sha: &str) -> Option<(String, String)> {
        let v = self.get_json(&format!(
            "https://api.github.com/repos/{slug}/commits/{sha}"
        ))?;
        let author = v.get("author")?;
        let login = author.get("login")?.as_str()?.to_string();
        let avatar = author
            .get("avatar_url")
            .and_then(|a| a.as_str())
            .map(String::from)
            .unwrap_or_else(|| format!("https://avatars.githubusercontent.com/{login}"));
        Some((login, avatar))
    }

    /// Fetch a user profile: (display name, company/affiliation).
    fn user_profile(&self, login: &str) -> Profile {
        let Some(v) = self.get_json(&format!("https://api.github.com/users/{login}")) else {
            return (None, None);
        };
        let name = v
            .get("name")
            .and_then(|n| n.as_str())
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(String::from);
        let company = v
            .get("company")
            .and_then(|c| c.as_str())
            .and_then(normalize_company);
        (name, company)
    }

    /// List every repository under a GitHub org or user, returning `owner/repo`
    /// slugs. Forks are skipped; archived repos are kept. Tries the org endpoint
    /// first and falls back to the user endpoint, so it works for either kind of
    /// account. Returns an empty vec if the owner can't be listed.
    pub fn list_owner_repos(&self, owner: &str) -> Vec<String> {
        for kind in ["orgs", "users"] {
            let mut slugs = Vec::new();
            let mut page = 1;
            let mut reached = false;
            loop {
                let url =
                    format!("https://api.github.com/{kind}/{owner}/repos?per_page=100&page={page}");
                let Some(v) = self.get_json(&url) else { break };
                reached = true;
                let Some(arr) = v.as_array() else { break };
                let count = arr.len();
                for repo in arr {
                    if repo.get("fork").and_then(|f| f.as_bool()).unwrap_or(false) {
                        continue;
                    }
                    if let Some(full) = repo.get("full_name").and_then(|n| n.as_str()) {
                        slugs.push(full.to_string());
                    }
                }
                if count < 100 {
                    break;
                }
                page += 1;
            }
            if reached && !slugs.is_empty() {
                return slugs;
            }
        }
        Vec::new()
    }

    pub fn fetch_bytes(&self, url: &str) -> Option<(Vec<u8>, String)> {
        let resp = self.agent.get(url).call().ok()?;
        let ct = resp.content_type().to_string();
        let mut buf = Vec::new();
        use std::io::Read;
        resp.into_reader()
            .take(4 * 1024 * 1024)
            .read_to_end(&mut buf)
            .ok()?;
        Some((buf, ct))
    }
}

/// Fill in `login` / `avatar_url` on clusters. Noreply emails resolve
/// offline; the rest are looked up via the commits API (one representative
/// commit per cluster), in parallel. `source_slugs` maps a commit's `src`
/// index to the `owner/repo` slug to query (or `None` for non-GitHub sources).
pub fn enrich_clusters(
    clusters: &mut [Cluster],
    commits: &[Commit],
    source_slugs: &[Option<String>],
    client: &GhClient,
    caches: &mut Caches,
    verbose: bool,
) {
    let slug_of = |c: &Commit| -> Option<&str> {
        source_slugs.get(c.src as usize).and_then(|s| s.as_deref())
    };
    let mut need_api: Vec<(usize, String, String)> = Vec::new();
    for (i, cl) in clusters.iter_mut().enumerate() {
        for email in &cl.emails {
            if let Some((id, login)) = parse_noreply(email) {
                cl.avatar_url = Some(match id {
                    Some(id) => format!("https://avatars.githubusercontent.com/u/{id}?v=4"),
                    None => format!("https://avatars.githubusercontent.com/{login}"),
                });
                cl.login = Some(login);
                break;
            }
        }
        if cl.login.is_none() {
            // Use the most recent commit that came from a GitHub source: old
            // commits are more likely to have stale email → account mappings.
            let rep = cl
                .commit_idxs
                .iter()
                .filter(|&&i| slug_of(&commits[i]).is_some())
                .max_by_key(|&&i| commits[i].ts);
            if let Some(&idx) = rep {
                let slug = slug_of(&commits[idx]).unwrap().to_string();
                need_api.push((i, slug, commits[idx].sha.clone()));
            }
        }
    }

    // A commit's author never changes, so resolved SHAs are cached forever.
    let mut from_cache = 0usize;
    need_api.retain(|(idx, _, sha)| match caches.author(sha) {
        Some(a) => {
            clusters[*idx].login = Some(a.login);
            clusters[*idx].avatar_url = Some(a.avatar_url);
            from_cache += 1;
            false
        }
        None => true,
    });

    if need_api.is_empty() || !client.has_token() {
        if !need_api.is_empty() && verbose {
            eprintln!(
                "  no GitHub token found ({} identities left unresolved) — run `gh auth login` to enable lookups",
                need_api.len()
            );
        }
        if from_cache > 0 && verbose {
            eprintln!("  resolved {from_cache} identities from cache");
        }
        return;
    }

    let cursor = AtomicUsize::new(0);
    let results: Mutex<HashMap<usize, (String, String)>> = Mutex::new(HashMap::new());
    std::thread::scope(|s| {
        for _ in 0..THREADS.min(need_api.len()) {
            s.spawn(|| loop {
                let i = cursor.fetch_add(1, Ordering::Relaxed);
                let Some((cluster_idx, slug, sha)) = need_api.get(i) else {
                    break;
                };
                if let Some(found) = client.commit_author(slug, sha) {
                    results.lock().unwrap().insert(*cluster_idx, found);
                }
            });
        }
    });

    let results = results.into_inner().unwrap();
    let resolved = results.len();
    // Map cluster index back to its representative SHA so resolved authors can
    // be cached against the (immutable) commit.
    let sha_of: HashMap<usize, &str> = need_api
        .iter()
        .map(|(idx, _, sha)| (*idx, sha.as_str()))
        .collect();
    for (idx, (login, avatar)) in results {
        if let Some(sha) = sha_of.get(&idx) {
            caches.put_author(sha.to_string(), login.clone(), avatar.clone());
        }
        clusters[idx].login = Some(login);
        clusters[idx].avatar_url = Some(avatar);
    }
    if verbose {
        eprintln!(
            "  resolved {resolved}/{} identities via GitHub API{}",
            need_api.len(),
            if from_cache > 0 {
                format!(" ({from_cache} more from cache)")
            } else {
                String::new()
            }
        );
    }
}

/// Fetch a repository's description from the GitHub API, if it has one.
pub fn fetch_repo_description(client: &GhClient, slug: &str) -> Option<String> {
    let v = client.get_json(&format!("https://api.github.com/repos/{slug}"))?;
    v.get("description")
        .and_then(|d| d.as_str())
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(String::from)
}

/// Fetch a GitHub avatar (e.g. an org/owner) and return it as a data URI,
/// caching the result by login and size.
pub fn fetch_avatar(
    client: &GhClient,
    caches: &mut Caches,
    login: &str,
    size: u32,
) -> Option<String> {
    let key = format!("owner:{login}:{size}");
    if let Some(data) = caches.avatar(&key) {
        return Some(data);
    }
    let url = format!("https://avatars.githubusercontent.com/{login}?s={size}");
    let (bytes, ct) = client.fetch_bytes(&url)?;
    let ct = if ct.starts_with("image/") {
        ct
    } else {
        "image/png".into()
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let data = format!("data:{ct};base64,{b64}");
    caches.put_avatar(key, data.clone());
    Some(data)
}

/// Clean up the free-text GitHub `company` field into a usable group name.
/// Handles common patterns like "@seqeralabs", "QBiC @qbicsoftware", and
/// multi-affiliation strings ("Seqera | SciLifeLab" → "Seqera").
pub fn normalize_company(raw: &str) -> Option<String> {
    let mut s = raw.trim();
    // Multiple affiliations: keep the first one.
    for sep in [" | ", " / ", ";", " · ", ","] {
        if let Some(i) = s.find(sep) {
            s = &s[..i];
        }
    }
    s = s.trim().trim_start_matches('@').trim();
    // "Company @githuborg" → "Company".
    if let Some(i) = s.find(" @") {
        s = &s[..i];
    }
    let s = s.trim().trim_end_matches(['.', ',', ';', '|', '/']).trim();
    if s.is_empty() || s.chars().count() > 60 {
        return None;
    }
    Some(s.to_string())
}

/// Fetch GitHub user profiles for every resolved login: improves display
/// names ("phue" → "Patrick Hüther") and yields company affiliations.
pub fn fetch_profiles(
    clusters: &mut [Cluster],
    client: &GhClient,
    caches: &mut Caches,
    verbose: bool,
) {
    let mut logins: Vec<String> = clusters.iter().filter_map(|c| c.login.clone()).collect();
    logins.sort();
    logins.dedup();
    if logins.is_empty() {
        return;
    }

    // Cached profiles skip the API; only the misses are fetched.
    let mut profiles: HashMap<String, Profile> = HashMap::new();
    let mut to_fetch: Vec<String> = Vec::new();
    for login in logins {
        match caches.profile(&login) {
            Some(p) => {
                profiles.insert(login, p);
            }
            None => to_fetch.push(login),
        }
    }
    let from_cache = profiles.len();

    if !to_fetch.is_empty() && client.has_token() {
        let cursor = AtomicUsize::new(0);
        let results: Mutex<HashMap<String, Profile>> = Mutex::new(HashMap::new());
        std::thread::scope(|s| {
            for _ in 0..THREADS.min(to_fetch.len()) {
                s.spawn(|| loop {
                    let i = cursor.fetch_add(1, Ordering::Relaxed);
                    let Some(login) = to_fetch.get(i) else { break };
                    let profile = client.user_profile(login);
                    results.lock().unwrap().insert(login.clone(), profile);
                });
            }
        });
        for (login, (name, company)) in results.into_inner().unwrap() {
            caches.put_profile(login.clone(), name.clone(), company.clone());
            profiles.insert(login, (name, company));
        }
    }

    let with_company = profiles.values().filter(|(_, c)| c.is_some()).count();
    for cl in clusters.iter_mut() {
        if let Some(login) = &cl.login {
            if let Some((name, company)) = profiles.get(login) {
                cl.profile_name = name.clone();
                cl.affiliation = company.clone();
            }
        }
    }
    if verbose {
        eprintln!(
            "  fetched {} profiles ({} with an affiliation, {from_cache} from cache)",
            profiles.len(),
            with_company,
        );
    }
}

/// Replace remote avatar URLs with embedded data URIs so the outputs are
/// fully self-contained (and render in places that block remote images).
pub fn embed_avatars(
    contributors: &mut [Contributor],
    client: &GhClient,
    caches: &mut Caches,
    size: u32,
    verbose: bool,
) {
    let mut urls: Vec<String> = Vec::new();
    for c in contributors.iter() {
        if let Some(u) = &c.avatar {
            if !u.starts_with("data:") && !urls.contains(u) {
                urls.push(u.clone());
            }
        }
    }
    if urls.is_empty() {
        return;
    }

    // Cache by avatar URL and size. Cached images skip the download.
    let key_of = |u: &str| format!("{u}|{size}");
    let mut embedded: HashMap<String, String> = HashMap::new();
    let mut to_fetch: Vec<String> = Vec::new();
    for u in urls {
        match caches.avatar(&key_of(&u)) {
            Some(data) => {
                embedded.insert(u, data);
            }
            None => to_fetch.push(u),
        }
    }
    let from_cache = embedded.len();

    if !to_fetch.is_empty() {
        let sized: Vec<String> = to_fetch
            .iter()
            .map(|u| {
                if u.contains('?') {
                    format!("{u}&s={size}")
                } else {
                    format!("{u}?s={size}")
                }
            })
            .collect();
        let cursor = AtomicUsize::new(0);
        let results: Mutex<HashMap<String, String>> = Mutex::new(HashMap::new());
        std::thread::scope(|s| {
            for _ in 0..THREADS.min(to_fetch.len()) {
                s.spawn(|| loop {
                    let i = cursor.fetch_add(1, Ordering::Relaxed);
                    let (Some(orig), Some(fetch_url)) = (to_fetch.get(i), sized.get(i)) else {
                        break;
                    };
                    if let Some((bytes, ct)) = client.fetch_bytes(fetch_url) {
                        let ct = if ct.starts_with("image/") {
                            ct
                        } else {
                            "image/png".into()
                        };
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                        results
                            .lock()
                            .unwrap()
                            .insert(orig.clone(), format!("data:{ct};base64,{b64}"));
                    }
                });
            }
        });
        for (url, data) in results.into_inner().unwrap() {
            caches.put_avatar(key_of(&url), data.clone());
            embedded.insert(url, data);
        }
    }

    let n = embedded.len();
    for c in contributors.iter_mut() {
        if let Some(u) = &c.avatar {
            if let Some(data) = embedded.get(u) {
                c.avatar = Some(data.clone());
            }
        }
    }
    if verbose {
        eprintln!("  embedded {n} avatars as data URIs ({from_cache} from cache)");
    }
}
