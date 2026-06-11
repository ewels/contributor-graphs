use crate::model::{month_index, Commit, Contributor};
use std::collections::HashMap;

/// A cluster of commit identities believed to be one person.
#[derive(Debug, Default, Clone)]
pub struct Cluster {
    pub emails: Vec<String>,
    pub names: Vec<String>,
    pub commit_idxs: Vec<usize>,
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

/// Group commits into identity clusters. Commits sharing an email always
/// merge; commits sharing a normalised author name merge unless disabled.
pub fn cluster_commits(commits: &[Commit], merge_names: bool) -> Vec<Cluster> {
    let mut dsu = Dsu::new();
    let mut by_email: HashMap<&str, usize> = HashMap::new();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let mut commit_node: Vec<usize> = Vec::with_capacity(commits.len());

    for c in commits {
        let key: &str = if c.email.is_empty() {
            &c.name
        } else {
            &c.email
        };
        let node = match by_email.get(key) {
            Some(&n) => n,
            None => {
                let n = dsu.make();
                by_email.insert(key, n);
                n
            }
        };
        if merge_names {
            let nn = norm_name(&c.name);
            if !nn.is_empty() {
                match by_name.get(&nn) {
                    Some(&other) => dsu.union(node, other),
                    None => {
                        by_name.insert(nn, node);
                    }
                }
            }
        }
        commit_node.push(node);
    }

    let mut clusters: Vec<Cluster> = Vec::new();
    let mut root_to_cluster: HashMap<usize, usize> = HashMap::new();
    for (i, c) in commits.iter().enumerate() {
        let root = dsu.find(commit_node[i]);
        let ci = *root_to_cluster.entry(root).or_insert_with(|| {
            clusters.push(Cluster::default());
            clusters.len() - 1
        });
        let cl = &mut clusters[ci];
        if !c.email.is_empty() && !cl.emails.iter().any(|e| e == &c.email) {
            cl.emails.push(c.email.clone());
        }
        if !c.name.is_empty() && !cl.names.iter().any(|n| n == &c.name) {
            cl.names.push(c.name.clone());
        }
        cl.commit_idxs.push(i);
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

pub fn is_bot(cl: &Cluster) -> bool {
    let hit = |s: &str| {
        let l = s.to_lowercase();
        l.contains("[bot]") || BOT_NAMES.contains(&l.as_str())
    };
    cl.names.iter().any(|n| hit(n))
        || cl.login.as_deref().is_some_and(hit)
        || cl.emails.iter().any(|e| {
            e.contains("[bot]@") || e.starts_with("actions@github.com") || e.contains("dependabot")
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

/// Build final contributors with stats and monthly activity bins.
pub fn build_contributors(
    clusters: &[Cluster],
    commits: &[Commit],
    groups: &[(String, String)],
) -> Vec<Contributor> {
    let mut out = Vec::with_capacity(clusters.len());
    for cl in clusters {
        if cl.commit_idxs.is_empty() {
            continue;
        }
        let mut first = i64::MAX;
        let mut last = i64::MIN;
        for &i in &cl.commit_idxs {
            first = first.min(commits[i].ts);
            last = last.max(commits[i].ts);
        }
        let m0 = month_index(first);
        let m1 = month_index(last);
        // Clamp the span so a single corrupt/extreme commit date can't trigger
        // a huge allocation (commits outside the window are simply not binned).
        let mut months = vec![0u32; (m1 - m0 + 1).clamp(1, 6000) as usize];
        for &i in &cl.commit_idxs {
            let mi = month_index(commits[i].ts) - m0;
            if let Some(slot) = months.get_mut(mi as usize) {
                *slot += 1;
            }
        }
        let name = cl
            .profile_name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| display_name(cl, commits));
        // Manual group mapping wins over auto-detected affiliation.
        let group = groups
            .iter()
            .find(|(matcher, _)| cluster_matches(cl, matcher))
            .map(|(_, g)| g.clone())
            .or_else(|| cl.affiliation.clone());
        let url = cl.login.as_ref().map(|l| format!("https://github.com/{l}"));
        out.push(Contributor {
            name,
            login: cl.login.clone(),
            avatar: cl.avatar_url.clone(),
            url,
            first,
            last,
            commits: cl.commit_idxs.len() as u32,
            bot: is_bot(cl),
            group,
            members: 1,
            member_names: Vec::new(),
            m0,
            months,
        });
    }
    out
}
