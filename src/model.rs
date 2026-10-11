use chrono::{DateTime, Datelike, Local, NaiveDate};
use std::collections::BTreeMap;

/// The reporting window, stored as local calendar dates (inclusive on both ends).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Period {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

impl Period {
    pub fn month(year: i32, month: u32) -> Self {
        let from = NaiveDate::from_ymd_opt(year, month, 1).expect("valid month");
        let next = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1)
        }
        .expect("valid month");
        Self {
            from,
            to: next.pred_opt().expect("valid date"),
        }
    }

    /// Start of `from` in local time, RFC 3339.
    pub fn start_rfc3339(&self) -> String {
        local_rfc3339(self.from, 0, 0, 0)
    }

    /// End of `to` in local time, RFC 3339.
    pub fn end_rfc3339(&self) -> String {
        local_rfc3339(self.to, 23, 59, 59)
    }

    pub fn contains(&self, t: &DateTime<Local>) -> bool {
        let d = t.date_naive();
        d >= self.from && d <= self.to
    }

    pub fn is_full_month(&self) -> bool {
        *self == Period::month(self.from.year(), self.from.month())
    }

    pub fn label(&self) -> String {
        if self.is_full_month() {
            self.from.format("%B %Y").to_string()
        } else {
            format!("{} → {}", self.from, self.to)
        }
    }

    /// Short slug for file names: `2026-09` or `2026-09-01_2026-09-15`.
    pub fn slug(&self) -> String {
        if self.is_full_month() {
            self.from.format("%Y-%m").to_string()
        } else {
            format!("{}_{}", self.from, self.to)
        }
    }
}

fn local_rfc3339(d: NaiveDate, h: u32, m: u32, s: u32) -> String {
    let naive = d.and_hms_opt(h, m, s).expect("valid time");
    match naive.and_local_timezone(Local).earliest() {
        Some(t) => t.to_rfc3339(),
        None => format!("{}Z", naive.format("%Y-%m-%dT%H:%M:%S")),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrState {
    Open,
    Draft,
    Merged,
    Closed,
}

impl PrState {
    pub fn label(self) -> &'static str {
        match self {
            PrState::Open => "open",
            PrState::Draft => "draft",
            PrState::Merged => "merged",
            PrState::Closed => "closed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PullRequest {
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: PrState,
    pub created_at: DateTime<Local>,
    pub merged_at: Option<DateTime<Local>>,
    /// Opened inside the period.
    pub opened_in_period: bool,
    /// Merged inside the period.
    pub merged_in_period: bool,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Debug, Clone)]
pub struct Review {
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub pr_author: String,
    /// Review states submitted in the period (APPROVED, CHANGES_REQUESTED, COMMENTED...).
    pub states: Vec<String>,
    pub last_at: DateTime<Local>,
}

impl Review {
    pub fn summary(&self) -> String {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for s in &self.states {
            let label = match s.as_str() {
                "APPROVED" => "approved",
                "CHANGES_REQUESTED" => "changes requested",
                "COMMENTED" => "commented",
                "DISMISSED" => "dismissed",
                _ => "other",
            };
            *counts.entry(label).or_default() += 1;
        }
        counts
            .iter()
            .map(|(k, v)| {
                if *v > 1 {
                    format!("{k} ×{v}")
                } else {
                    k.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: String,
    pub created_at: DateTime<Local>,
}

/// Issues or PRs the user commented on during the period (outside of formal reviews).
#[derive(Debug, Clone)]
pub struct Discussion {
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub is_pr: bool,
    pub comments: usize,
    pub last_at: DateTime<Local>,
}

#[derive(Debug, Clone, Default)]
pub struct Activity {
    pub user: String,
    pub pull_requests: Vec<PullRequest>,
    pub reviews: Vec<Review>,
    pub issues: Vec<Issue>,
    pub discussions: Vec<Discussion>,
    /// Commits to default branches, per repo (as counted by GitHub's contribution graph).
    pub commits: BTreeMap<String, u64>,
    /// Non-fatal problems (e.g. comment lookups that failed).
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RepoStats {
    pub name: String,
    pub prs: usize,
    pub reviews: usize,
    pub issues: usize,
    pub discussions: usize,
    pub commits: u64,
}

impl RepoStats {
    pub fn total(&self) -> u64 {
        (self.prs + self.reviews + self.issues + self.discussions) as u64 + self.commits
    }
}

impl Activity {
    /// Repos touched in the period, most active first.
    pub fn repos(&self) -> Vec<RepoStats> {
        let mut map: BTreeMap<String, RepoStats> = BTreeMap::new();
        fn entry<'a>(map: &'a mut BTreeMap<String, RepoStats>, r: &str) -> &'a mut RepoStats {
            map.entry(r.to_string()).or_insert_with(|| RepoStats {
                name: r.to_string(),
                ..Default::default()
            })
        }
        for p in &self.pull_requests {
            entry(&mut map, &p.repo).prs += 1;
        }
        for r in &self.reviews {
            entry(&mut map, &r.repo).reviews += 1;
        }
        for i in &self.issues {
            entry(&mut map, &i.repo).issues += 1;
        }
        for d in &self.discussions {
            entry(&mut map, &d.repo).discussions += 1;
        }
        for (repo, n) in &self.commits {
            entry(&mut map, repo).commits += n;
        }
        let mut v: Vec<_> = map.into_values().collect();
        v.sort_by(|a, b| b.total().cmp(&a.total()).then(a.name.cmp(&b.name)));
        v
    }

    /// A copy restricted to the given repos.
    pub fn filtered(&self, repos: &[String]) -> Activity {
        let keep = |r: &String| repos.contains(r);
        Activity {
            user: self.user.clone(),
            pull_requests: self
                .pull_requests
                .iter()
                .filter(|x| keep(&x.repo))
                .cloned()
                .collect(),
            reviews: self
                .reviews
                .iter()
                .filter(|x| keep(&x.repo))
                .cloned()
                .collect(),
            issues: self
                .issues
                .iter()
                .filter(|x| keep(&x.repo))
                .cloned()
                .collect(),
            discussions: self
                .discussions
                .iter()
                .filter(|x| keep(&x.repo))
                .cloned()
                .collect(),
            commits: self
                .commits
                .iter()
                .filter(|(r, _)| keep(r))
                .map(|(r, n)| (r.clone(), *n))
                .collect(),
            warnings: self.warnings.clone(),
        }
    }
}
