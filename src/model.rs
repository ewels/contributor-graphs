use serde::Serialize;

/// A single commit as parsed from `git log`.
#[derive(Debug, Clone)]
pub struct Commit {
    pub sha: String,
    pub ts: i64,
    pub name: String,
    pub email: String,
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
    pub months: Vec<u32>,
}

/// Collapse contributors into one row per affiliation. People without a
/// detected group fall into a single bucket labelled `unaffiliated`.
pub fn aggregate_by_group(contributors: &[Contributor], unaffiliated: &str) -> Vec<Contributor> {
    use std::collections::HashMap;

    struct Agg {
        commits: u32,
        first: i64,
        last: i64,
        months: HashMap<i32, u32>,
        members: Vec<(String, u32)>,
    }
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, Agg> = HashMap::new();

    for c in contributors {
        let key = c.group.clone().unwrap_or_else(|| unaffiliated.to_string());
        let agg = map.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Agg {
                commits: 0,
                first: i64::MAX,
                last: i64::MIN,
                months: HashMap::new(),
                members: Vec::new(),
            }
        });
        agg.commits += c.commits;
        agg.first = agg.first.min(c.first);
        agg.last = agg.last.max(c.last);
        agg.members.push((c.name.clone(), c.commits));
        for (i, &v) in c.months.iter().enumerate() {
            if v > 0 {
                *agg.months.entry(c.m0 + i as i32).or_insert(0) += v;
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
