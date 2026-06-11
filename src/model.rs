use serde::Serialize;

/// One manual `matcher -> group` rule, optionally limited to a date window.
/// Several rules may share a matcher to give a contributor different
/// affiliations over time; where windows overlap the later `since` wins.
#[derive(Debug, Clone)]
pub struct GroupRule {
    pub matcher: String,
    pub group: String,
    /// Inclusive start (unix seconds); `None` means open at the start.
    pub since: Option<i64>,
    /// Exclusive end (unix seconds); `None` means open at the end.
    pub until: Option<i64>,
}

impl GroupRule {
    /// Whether this rule's window contains a commit at `ts`.
    pub fn covers(&self, ts: i64) -> bool {
        self.since.is_none_or(|s| ts >= s) && self.until.is_none_or(|u| ts < u)
    }
    pub fn dated(&self) -> bool {
        self.since.is_some() || self.until.is_some()
    }
}

/// The `git log` filters that affect which commits a source yields. Grouped so
/// they travel together and form part of the history cache key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitFilter {
    pub since: Option<String>,
    pub until: Option<String>,
    pub no_merges: bool,
}

/// A single commit as parsed from `git log`.
#[derive(Debug, Clone)]
pub struct Commit {
    pub sha: String,
    pub ts: i64,
    pub name: String,
    pub email: String,
    /// `(name, email)` of each `Co-authored-by` trailer on the commit.
    pub coauthors: Vec<(String, String)>,
    /// Index of the source this commit came from (see `analyze_many`). 0 for a
    /// single-source run.
    pub src: u32,
}

/// One merged contributor identity, ready for rendering. Also reused for
/// affiliation aggregates, where one "row" stands for a whole organisation.
#[derive(Debug, Clone, Serialize)]
pub struct Contributor {
    pub name: String,
    pub login: Option<String>,
    pub avatar: Option<String>,
    pub url: Option<String>,
    pub first: i64,
    pub last: i64,
    pub commits: u32,
    pub bot: bool,
    pub group: Option<String>,
    /// Number of people behind this row (1 for an individual; N for an
    /// affiliation aggregate).
    pub members: u32,
    /// Names of the largest contributors in an aggregate (for tooltips).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub member_names: Vec<String>,
    /// Month index (months since 1970-01) of the first entry in `months`.
    pub m0: i32,
    /// Commits per calendar month, from `m0` through the last active month.
    /// Includes co-authored commits (full credit); `co_months` is the subset
    /// so the interactive page can subtract it when co-authors are toggled off.
    pub months: Vec<u32>,
    /// Co-authored commits per month, aligned to `m0` (a subset of `months`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub co_months: Vec<u32>,
    /// Of `commits`, how many the person was a co-author on (not the author).
    #[serde(skip_serializing_if = "is_zero")]
    pub co_commits: u32,
    /// Per-month affiliation, aligned to `m0`, when the person has time-bounded
    /// manual affiliations (so their row is coloured by org over time). `None`
    /// when a single affiliation applies throughout (the usual case).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub month_groups: Option<Vec<Option<String>>>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// Collapse contributors into one row per affiliation. People without a
/// detected group fall into a single bucket labelled `unaffiliated`.
pub fn aggregate_by_group(contributors: &[Contributor], unaffiliated: &str) -> Vec<Contributor> {
    use std::collections::HashMap;

    struct Agg {
        commits: u32,
        co_commits: u32,
        first: i64,
        last: i64,
        months: HashMap<i32, u32>,
        co_months: HashMap<i32, u32>,
        members: Vec<(String, u32)>,
    }
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, Agg> = HashMap::new();
    macro_rules! agg {
        ($key:expr) => {
            map.entry($key.clone()).or_insert_with(|| {
                order.push($key.clone());
                Agg {
                    commits: 0,
                    co_commits: 0,
                    first: i64::MAX,
                    last: i64::MIN,
                    months: HashMap::new(),
                    co_months: HashMap::new(),
                    members: Vec::new(),
                }
            })
        };
    }

    for c in contributors {
        let default_key = c.group.clone().unwrap_or_else(|| unaffiliated.to_string());
        match &c.month_groups {
            // Single affiliation throughout: the whole person joins one group.
            None => {
                let a = agg!(default_key);
                a.commits += c.commits;
                a.co_commits += c.co_commits;
                a.first = a.first.min(c.first);
                a.last = a.last.max(c.last);
                a.members.push((c.name.clone(), c.commits));
                for (i, &v) in c.months.iter().enumerate() {
                    if v > 0 {
                        *a.months.entry(c.m0 + i as i32).or_insert(0) += v;
                    }
                }
                for (i, &v) in c.co_months.iter().enumerate() {
                    if v > 0 {
                        *a.co_months.entry(c.m0 + i as i32).or_insert(0) += v;
                    }
                }
            }
            // Time-bounded affiliations: split each month into its active org.
            Some(mg) => {
                let key_at = |i: usize| {
                    mg.get(i)
                        .cloned()
                        .flatten()
                        .unwrap_or_else(|| default_key.clone())
                };
                let mut per_group: HashMap<String, u32> = HashMap::new();
                for (i, &v) in c.months.iter().enumerate() {
                    if v == 0 {
                        continue;
                    }
                    let key = key_at(i);
                    let m = c.m0 + i as i32;
                    let ts = month_start_ts(m);
                    let a = agg!(key);
                    *a.months.entry(m).or_insert(0) += v;
                    a.commits += v;
                    a.first = a.first.min(ts);
                    a.last = a.last.max(ts);
                    *per_group.entry(key).or_insert(0) += v;
                }
                for (i, &v) in c.co_months.iter().enumerate() {
                    if v == 0 {
                        continue;
                    }
                    let key = key_at(i);
                    let a = agg!(key);
                    *a.co_months.entry(c.m0 + i as i32).or_insert(0) += v;
                    a.co_commits += v;
                }
                for (key, n) in per_group {
                    agg!(key).members.push((c.name.clone(), n));
                }
            }
        }
    }

    order
        .into_iter()
        .map(|key| {
            let agg = map.remove(&key).unwrap();
            let m0 = *agg.months.keys().min().unwrap_or(&month_index(agg.first));
            let m1 = *agg.months.keys().max().unwrap_or(&m0);
            let len = (m1 - m0 + 1).clamp(1, 6000) as usize;
            let mut months = vec![0u32; len];
            for (&m, &v) in &agg.months {
                if let Some(slot) = months.get_mut((m - m0) as usize) {
                    *slot += v;
                }
            }
            let mut co_months = vec![0u32; if agg.co_months.is_empty() { 0 } else { len }];
            for (&m, &v) in &agg.co_months {
                if let Some(slot) = co_months.get_mut((m - m0) as usize) {
                    *slot += v;
                }
            }
            let mut members = agg.members;
            members.sort_by_key(|(_, commits)| std::cmp::Reverse(*commits));
            let member_count = members.len() as u32;
            let member_names = members.into_iter().take(8).map(|(n, _)| n).collect();
            Contributor {
                name: key.clone(),
                login: None,
                avatar: None,
                url: None,
                first: agg.first,
                last: agg.last,
                commits: agg.commits,
                bot: false,
                group: Some(key),
                members: member_count,
                member_names,
                m0,
                months,
                co_months,
                co_commits: agg.co_commits,
                month_groups: None,
            }
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoMeta {
    pub name: String,
    pub url: Option<String>,
    pub slug: Option<String>,
    pub branch: String,
    pub first: i64,
    pub last: i64,
    pub total_commits: u64,
    pub total_contributors: usize,
    pub generated: String,
    /// Owner/org avatar as a data URI, for the interactive page header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_avatar: Option<String>,
    /// The GitHub repository description, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

pub fn month_index(ts: i64) -> i32 {
    use chrono::{Datelike, TimeZone, Utc};
    let dt = Utc.timestamp_opt(ts, 0).single().unwrap_or_default();
    (dt.year() - 1970) * 12 + dt.month0() as i32
}

pub fn month_start_ts(mi: i32) -> i64 {
    use chrono::{TimeZone, Utc};
    let year = 1970 + mi.div_euclid(12);
    let month = mi.rem_euclid(12) as u32 + 1;
    Utc.with_ymd_and_hms(year, month, 1, 0, 0, 0)
        .single()
        .map(|d| d.timestamp())
        .unwrap_or_default()
}

pub fn format_month_year(ts: i64) -> String {
    use chrono::{TimeZone, Utc};
    Utc.timestamp_opt(ts, 0)
        .single()
        .map(|d| d.format("%b %Y").to_string())
        .unwrap_or_default()
}

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
