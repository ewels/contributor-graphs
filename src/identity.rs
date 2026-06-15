use crate::model::{month_index, month_start_ts, Commit, Contributor, GroupRule};
use std::collections::HashMap;

/// A cluster of commit identities believed to be one person.
#[derive(Debug, Default, Clone)]
pub struct Cluster {
    pub emails: Vec<String>,
    pub names: Vec<String>,
    /// Commits this person authored.
    pub commit_idxs: Vec<usize>,
    /// Commits this person was a `Co-authored-by` on (and did not author).
    pub coauthored_idxs: Vec<usize>,
    pub login: Option<String>,
    pub avatar_url: Option<String>,
    /// Display name from the GitHub user profile.
    pub profile_name: Option<String>,
    /// Affiliation from the GitHub profile `company` field.
    pub affiliation: Option<String>,
}

struct Dsu(Vec<usize>);

impl Dsu {
    fn new() -> Self {
        Dsu(Vec::new())
    }
    fn make(&mut self) -> usize {
        self.0.push(self.0.len());
        self.0.len() - 1
    }
    fn find(&mut self, x: usize) -> usize {
        if self.0[x] != x {
            let root = self.find(self.0[x]);
            self.0[x] = root;
        }
        self.0[x]
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.0[rb] = ra;
        }
    }
}

fn norm_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The graph node for an identity: keyed by email (or name when email-less),
/// unioned with any prior identity sharing a normalised name.
fn node_for(
    dsu: &mut Dsu,
    by_email: &mut HashMap<String, usize>,
    by_name: &mut HashMap<String, usize>,
    merge_names: bool,
    name: &str,
    email: &str,
) -> usize {
    let key = if email.is_empty() { name } else { email };
    let node = match by_email.get(key) {
        Some(&n) => n,
        None => {
            let n = dsu.make();
            by_email.insert(key.to_string(), n);
            n
        }
    };
    if merge_names {
        let nn = norm_name(name);
        if !nn.is_empty() {
            match by_name.get(&nn) {
                Some(&other) => dsu.union(node, other),
                None => {
                    by_name.insert(nn, node);
                }
            }
        }
    }
    node
}

fn add_identity(cl: &mut Cluster, name: &str, email: &str) {
    if !email.is_empty() && !cl.emails.iter().any(|e| e == email) {
        cl.emails.push(email.to_string());
    }
    if !name.is_empty() && !cl.names.iter().any(|n| n == name) {
        cl.names.push(name.to_string());
    }
}

fn cluster_index(
    dsu: &mut Dsu,
    map: &mut HashMap<usize, usize>,
    clusters: &mut Vec<Cluster>,
    node: usize,
) -> usize {
    let root = dsu.find(node);
    *map.entry(root).or_insert_with(|| {
        clusters.push(Cluster::default());
        clusters.len() - 1
    })
}

/// Group commit identities into clusters, one per person. Authors and their
/// `Co-authored-by` identities all share the graph (so a co-author merges with
/// the same person's authored commits); each cluster records the commits it
/// authored and, separately, those it only co-authored.
pub fn cluster_commits(commits: &[Commit], merge_names: bool) -> Vec<Cluster> {
    let mut dsu = Dsu::new();
    let mut by_email: HashMap<String, usize> = HashMap::new();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let mut author_node: Vec<usize> = Vec::with_capacity(commits.len());
    let mut coauthor_nodes: Vec<Vec<usize>> = Vec::with_capacity(commits.len());

    for c in commits {
        author_node.push(node_for(
            &mut dsu,
            &mut by_email,
            &mut by_name,
            merge_names,
            &c.name,
            &c.email,
        ));
        let cns = c
            .coauthors
            .iter()
            .map(|(n, e)| node_for(&mut dsu, &mut by_email, &mut by_name, merge_names, n, e))
            .collect();
        coauthor_nodes.push(cns);
    }

    let mut clusters: Vec<Cluster> = Vec::new();
    let mut root_to_cluster: HashMap<usize, usize> = HashMap::new();
    for (i, c) in commits.iter().enumerate() {
        let ci_a = cluster_index(
            &mut dsu,
            &mut root_to_cluster,
            &mut clusters,
            author_node[i],
        );
        add_identity(&mut clusters[ci_a], &c.name, &c.email);
        clusters[ci_a].commit_idxs.push(i);
        for (k, (n, e)) in c.coauthors.iter().enumerate() {
            let ci_c = cluster_index(
                &mut dsu,
                &mut root_to_cluster,
                &mut clusters,
                coauthor_nodes[i][k],
            );
            add_identity(&mut clusters[ci_c], n, e);
            // Skip when the co-author is the author, or already credited for
            // this commit (a duplicate trailer, or two of their aliases on it).
            if ci_c != ci_a && clusters[ci_c].coauthored_idxs.last() != Some(&i) {
                clusters[ci_c].coauthored_idxs.push(i);
            }
        }
    }
    clusters
}

/// Merge clusters that resolved to the same GitHub login.
pub fn merge_by_login(clusters: Vec<Cluster>) -> Vec<Cluster> {
    let mut by_login: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<Cluster> = Vec::new();
    for cl in clusters {
        if let Some(login) = cl.login.clone() {
            let key = login.to_lowercase();
            if let Some(&i) = by_login.get(&key) {
                merge_into(&mut out[i], cl);
                continue;
            }
            by_login.insert(key, out.len());
        }
        out.push(cl);
    }
    out
}

/// Apply a manual identity file: each TSV row lists a canonical name followed
/// by aliases. Any cluster whose name, email, or login matches any field is
/// merged, and the first field becomes the display name.
pub fn apply_identity_file(clusters: Vec<Cluster>, rows: &[Vec<String>]) -> Vec<Cluster> {
    let mut clusters: Vec<Option<Cluster>> = clusters.into_iter().map(Some).collect();
    for row in rows {
        if row.is_empty() {
            continue;
        }
        let canonical = &row[0];
        let matches: Vec<usize> = clusters
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let c = c.as_ref()?;
                let hit = row.iter().any(|alias| cluster_matches(c, alias));
                hit.then_some(i)
            })
            .collect();
        if matches.is_empty() {
            continue;
        }
        let target = matches[0];
        for &i in matches.iter().skip(1) {
            let donor = clusters[i].take().unwrap();
            let t = clusters[target].as_mut().unwrap();
            merge_into(t, donor);
        }
        let t = clusters[target].as_mut().unwrap();
        // Force the canonical display name to sort first.
        t.names.retain(|n| n != canonical);
        t.names.insert(0, canonical.clone());
    }
    clusters.into_iter().flatten().collect()
}

pub fn cluster_matches(c: &Cluster, needle: &str) -> bool {
    let n = needle.trim().to_lowercase();
    if n.is_empty() {
        return false;
    }
    c.emails.iter().any(|e| e.to_lowercase() == n)
        || c.names.iter().any(|name| name.to_lowercase() == n)
        || c.login.as_deref().is_some_and(|l| l.to_lowercase() == n)
}

fn merge_into(target: &mut Cluster, donor: Cluster) {
    for e in donor.emails {
        if !target.emails.contains(&e) {
            target.emails.push(e);
        }
    }
    for n in donor.names {
        if !target.names.contains(&n) {
            target.names.push(n);
        }
    }
    target.commit_idxs.extend(donor.commit_idxs);
    target.coauthored_idxs.extend(donor.coauthored_idxs);
    if target.login.is_none() {
        target.login = donor.login;
    }
    if target.avatar_url.is_none() {
        target.avatar_url = donor.avatar_url;
    }
    if target.profile_name.is_none() {
        target.profile_name = donor.profile_name;
    }
    if target.affiliation.is_none() {
        target.affiliation = donor.affiliation;
    }
}

const BOT_NAMES: &[&str] = &[
    "github-actions",
    "github actions",
    "dependabot",
    "renovate",
    "renovate bot",
    "greenkeeper",
    "snyk-bot",
    "travis ci user",
    "travis ci",
    "travis",
    "runner",
    "nf-core-bot",
    "semantic-release-bot",
    "allcontributors",
    "pre-commit-ci",
    "imgbot",
    "codecov",
    "whitesource",
    "deepsource",
    "pyup.io bot",
    "pyup-bot",
    "mergify",
    "copilot",
];

/// Fixed author/co-author addresses used by automated agents: CI/release bots
/// and LLM coding agents. Matched exactly (case-insensitively) so real people
/// at the same domain — name@anthropic.com, name@cursor.com, a Seqera engineer
/// who tags "+ Codex" onto their own name@seqera.io commits — stay human.
const BOT_EMAILS: &[&str] = &[
    "noreply@anthropic.com",  // Claude / Claude Code
    "cursoragent@cursor.com", // Cursor Agent
    "codex@openai.com",       // OpenAI Codex
    "noreply@opencode.ai",    // OpenCode
    "seqera-ai@seqera.io",    // Seqera AI
];

/// Substrings that mark an automated address whatever digits GitHub prepends to
/// the local part: any GitHub App (`…[bot]@…`) plus CI/release bots whose
/// addresses vary. Unlike `BOT_EMAILS` these match anywhere in the address.
const BOT_EMAIL_PATTERNS: &[&str] = &["[bot]@", "actions@github.com", "dependabot"];

pub fn is_bot(cl: &Cluster) -> bool {
    let hit = |s: &str| {
        let l = s.to_lowercase();
        l.contains("[bot]") || BOT_NAMES.contains(&l.as_str())
    };
    cl.names.iter().any(|n| hit(n))
        || cl.login.as_deref().is_some_and(hit)
        || cl.emails.iter().any(|e| {
            let e = e.to_lowercase();
            BOT_EMAILS.contains(&e.as_str()) || BOT_EMAIL_PATTERNS.iter().any(|p| e.contains(p))
        })
}

/// Pick the best human-readable display name for a cluster: the most frequent
/// author name, preferring "Firstname Lastname"-style over login-style names.
fn display_name(cl: &Cluster, commits: &[Commit]) -> String {
    let mut freq: HashMap<&str, (u32, usize)> = HashMap::new();
    for (order, &i) in cl.commit_idxs.iter().enumerate() {
        let name = commits[i].name.as_str();
        if name.is_empty() {
            continue;
        }
        let e = freq.entry(name).or_insert((0, order));
        e.0 += 1;
    }
    let score = |name: &str, count: u32| {
        let mut s = count as f64;
        if name.contains(' ') {
            s *= 3.0; // prefer full names over handles
        }
        if name.chars().next().is_some_and(|c| c.is_uppercase()) {
            s *= 1.5;
        }
        s
    };
    freq.iter()
        .max_by(|(a, (ca, oa)), (b, (cb, ob))| {
            score(a, *ca)
                .partial_cmp(&score(b, *cb))
                .unwrap()
                .then(ob.cmp(oa)) // earlier-seen wins ties
        })
        .map(|(n, _)| n.to_string())
        .unwrap_or_else(|| {
            cl.login.clone().unwrap_or_else(|| {
                cl.names
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "unknown".into())
            })
        })
}

/// A plausible GitHub username, so a name- or email-based matcher isn't turned
/// into a bogus profile link: ASCII alphanumerics and hyphens, at most 39 chars.
fn is_github_username(s: &str) -> bool {
    !s.is_empty() && s.len() <= 39 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Build final contributors with stats and monthly activity bins. With
/// `count_coauthors`, each `Co-authored-by` credit is counted alongside the
/// authored commits (and tracked separately in `co_months` / `co_commits`).
pub fn build_contributors(
    clusters: &[Cluster],
    commits: &[Commit],
    groups: &[GroupRule],
    forced_names: &[(String, String)],
    count_coauthors: bool,
) -> Vec<Contributor> {
    let mut out = Vec::with_capacity(clusters.len());
    for cl in clusters {
        let coauthored: &[usize] = if count_coauthors {
            &cl.coauthored_idxs
        } else {
            &[]
        };
        if cl.commit_idxs.is_empty() && coauthored.is_empty() {
            continue;
        }
        let mut first = i64::MAX;
        let mut last = i64::MIN;
        for &i in cl.commit_idxs.iter().chain(coauthored.iter()) {
            first = first.min(commits[i].ts);
            last = last.max(commits[i].ts);
        }
        let m0 = month_index(first);
        let m1 = month_index(last);
        // Clamp the span so a single corrupt/extreme commit date can't trigger
        // a huge allocation (commits outside the window are simply not binned).
        let len = (m1 - m0 + 1).clamp(1, 6000) as usize;
        let mut months = vec![0u32; len];
        // Only the co-authored rows allocate a second array.
        let mut co_months = vec![0u32; if coauthored.is_empty() { 0 } else { len }];
        for &i in &cl.commit_idxs {
            if let Some(slot) = months.get_mut((month_index(commits[i].ts) - m0) as usize) {
                *slot += 1;
            }
        }
        for &i in coauthored {
            let mi = (month_index(commits[i].ts) - m0) as usize;
            if let Some(slot) = months.get_mut(mi) {
                *slot += 1;
            }
            if let Some(slot) = co_months.get_mut(mi) {
                *slot += 1;
            }
        }
        // A name supplied in the curation TSV is authoritative; otherwise fall
        // back to the GitHub profile name, then the commit-derived name.
        let name = forced_names
            .iter()
            .find(|(matcher, _)| cluster_matches(cl, matcher))
            .map(|(_, name)| name.clone())
            .or_else(|| cl.profile_name.clone().filter(|n| !n.trim().is_empty()))
            .unwrap_or_else(|| display_name(cl, commits));
        // Manual group rules win over the auto-detected affiliation. With
        // dated rules, the person's months are coloured by the org active at
        // the time (later `since` wins overlaps); the primary `group` is their
        // most recent affiliation.
        let matching: Vec<&GroupRule> = groups
            .iter()
            .filter(|r| cluster_matches(cl, &r.matcher))
            .collect();
        let (group, month_groups) = if matching.is_empty() {
            (cl.affiliation.clone(), None)
        } else if !matching.iter().any(|r| r.dated()) {
            (Some(matching[0].group.clone()), None)
        } else {
            let active_at = |ts: i64| -> Option<&str> {
                matching
                    .iter()
                    .filter(|r| r.covers(ts))
                    .max_by_key(|r| r.since.unwrap_or(i64::MIN))
                    .map(|r| r.group.as_str())
            };
            let mg: Vec<Option<String>> = (0..len)
                .map(|mi| active_at(month_start_ts(m0 + mi as i32)).map(str::to_string))
                .collect();
            // Primary = the most recent affiliation the person was actually
            // active under (their last coloured month), so an affiliation that
            // starts after they stopped committing doesn't mislabel them. Fall
            // back to the latest rule if no active month is covered at all.
            let primary = mg.iter().rev().flatten().next().cloned().or_else(|| {
                matching
                    .iter()
                    .max_by_key(|r| r.since.unwrap_or(i64::MIN))
                    .map(|r| r.group.clone())
            });
            let month_groups = mg.iter().any(|g| g.is_some()).then_some(mg);
            (primary, month_groups)
        };
        // If commit metadata never resolved a GitHub account but a manual
        // affiliation row matched (on name/email), adopt that row's username as
        // the login. GitHub serves the avatar and profile page by username, so
        // an otherwise-accountless contributor gets their picture and link with
        // no extra API call.
        let backfill: Option<&str> = cl
            .login
            .is_none()
            .then(|| {
                matching
                    .iter()
                    .map(|r| r.matcher.as_str())
                    .find(|&m| is_github_username(m))
            })
            .flatten();
        let login = cl.login.clone().or_else(|| backfill.map(str::to_string));
        let avatar = cl
            .avatar_url
            .clone()
            .or_else(|| backfill.map(|u| format!("https://avatars.githubusercontent.com/{u}")));
        let url = login.as_ref().map(|l| format!("https://github.com/{l}"));
        out.push(Contributor {
            name,
            login,
            avatar,
            url,
            first,
            last,
            commits: (cl.commit_idxs.len() + coauthored.len()) as u32,
            bot: is_bot(cl),
            group,
            members: 1,
            member_names: Vec::new(),
            m0,
            months,
            co_months,
            co_commits: coauthored.len() as u32,
            month_groups,
        });
    }
    out
}
