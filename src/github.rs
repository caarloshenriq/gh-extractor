//! GitHub data collection.
//!
//! Main source is the GraphQL `contributionsCollection`, which is what GitHub
//! itself uses for the contribution graph: PRs opened, reviews, issues opened and
//! default-branch commits inside the window. On top of that we add:
//!   * PRs merged in the window (opened earlier, so not in the collection),
//!   * issue/PR conversation comments in the window (verified per thread).

use crate::model::*;
use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Local};
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Mutex;

const API: &str = "https://api.github.com";
const MAX_DISCUSSION_THREADS: usize = 200;
const COMMENT_WORKERS: usize = 8;

pub struct GitHub {
    client: Client,
    token: String,
}

/// Looks for a token in GITHUB_TOKEN, GH_TOKEN, then `gh auth token`.
pub fn find_token() -> Option<String> {
    for var in ["GITHUB_TOKEN", "GH_TOKEN"] {
        if let Ok(t) = std::env::var(var)
            && !t.trim().is_empty()
        {
            return Some(t.trim().to_string());
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

impl GitHub {
    pub fn new(token: String) -> Result<Self> {
        let client = Client::builder()
            .user_agent(concat!("boss-report/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self { client, token })
    }

    fn graphql(&self, query: &str, vars: Value) -> Result<Value> {
        let resp = self
            .client
            .post(format!("{API}/graphql"))
            .bearer_auth(&self.token)
            .json(&json!({ "query": query, "variables": vars }))
            .send()
            .context("GitHub GraphQL request failed")?;
        let status = resp.status();
        let body: Value = resp.json().context("invalid JSON from GitHub")?;
        if !status.is_success() {
            bail!(
                "GitHub returned {status}: {}",
                body["message"].as_str().unwrap_or("unknown error")
            );
        }
        if let Some(errs) = body["errors"].as_array()
            && !errs.is_empty()
        {
            let msgs: Vec<_> = errs.iter().filter_map(|e| e["message"].as_str()).collect();
            bail!("GitHub GraphQL error: {}", msgs.join("; "));
        }
        Ok(body["data"].clone())
    }

    fn rest(&self, path: &str) -> Result<Value> {
        let resp = self
            .client
            .get(format!("{API}{path}"))
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .send()?;
        let status = resp.status();
        let body: Value = resp.json()?;
        if !status.is_success() {
            bail!("{status}: {}", body["message"].as_str().unwrap_or("error"));
        }
        Ok(body)
    }

    /// Login of the token owner.
    pub fn viewer(&self) -> Result<String> {
        let d = self.graphql("query { viewer { login } }", json!({}))?;
        d["viewer"]["login"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow!("could not read viewer login"))
    }

    /// Collects everything `user` did in `period`. `progress` receives status lines.
    pub fn fetch(
        &self,
        user: &str,
        period: Period,
        progress: &(dyn Fn(String) + Sync),
    ) -> Result<Activity> {
        if period.to < period.from {
            bail!("'To' date is before 'From' date");
        }
        if (period.to - period.from).num_days() > 365 {
            bail!("GitHub only allows ranges up to one year");
        }
        let from = period.start_rfc3339();
        let to = period.end_rfc3339();
        let mut act = Activity {
            user: user.to_string(),
            ..Default::default()
        };

        // ---- Commits + sanity check that the user exists ----
        progress("Checking user and commit contributions…".into());
        let d = self.graphql(
            r#"query($login:String!,$from:DateTime!,$to:DateTime!){
              user(login:$login){ login contributionsCollection(from:$from,to:$to){
                restrictedContributionsCount
                commitContributionsByRepository(maxRepositories:100){
                  repository{ nameWithOwner } contributions{ totalCount } } } } }"#,
            json!({ "login": user, "from": from, "to": to }),
        )?;
        if d["user"].is_null() {
            bail!("GitHub user '{user}' not found");
        }
        let cc = &d["user"]["contributionsCollection"];
        for c in arr(&cc["commitContributionsByRepository"]) {
            let repo = s(&c["repository"]["nameWithOwner"]);
            let n = c["contributions"]["totalCount"].as_u64().unwrap_or(0);
            if n > 0 {
                act.commits.insert(repo, n);
            }
        }
        let restricted = cc["restrictedContributionsCount"].as_u64().unwrap_or(0);
        if restricted > 0 {
            act.warnings.push(format!(
                "{restricted} private contribution(s) are hidden by GitHub and not listed."
            ));
        }

        // ---- PRs opened in the period ----
        progress("Fetching pull requests opened…".into());
        let mut prs: BTreeMap<String, PullRequest> = BTreeMap::new();
        let nodes = self.paginate_collection(
            user,
            &from,
            &to,
            "pullRequestContributions",
            &format!("pullRequest {{ {PR_FIELDS} }}"),
        )?;
        for n in nodes {
            if let Some(pr) = parse_pr(&n["pullRequest"], &period) {
                prs.insert(pr.url.clone(), pr);
            }
        }

        // ---- PRs merged in the period (may have been opened earlier) ----
        progress("Fetching pull requests merged…".into());
        let q = format!("is:pr author:{user} merged:{from}..{to}");
        for n in self.paginate_search(&q, &format!("... on PullRequest {{ {PR_FIELDS} }}"), 1000)? {
            if let Some(pr) = parse_pr(&n, &period) {
                prs.entry(pr.url.clone()).or_insert(pr);
            }
        }
        act.pull_requests = prs.into_values().collect();
        act.pull_requests
            .sort_by(|a, b| a.repo.cmp(&b.repo).then(a.number.cmp(&b.number)));

        // ---- Reviews ----
        progress("Fetching reviews…".into());
        let nodes = self.paginate_collection(
            user,
            &from,
            &to,
            "pullRequestReviewContributions",
            "occurredAt pullRequestReview { state } pullRequest { number title url author { login } repository { nameWithOwner } }",
        )?;
        let mut reviews: BTreeMap<String, Review> = BTreeMap::new();
        for n in nodes {
            let pr = &n["pullRequest"];
            let url = s(&pr["url"]);
            let at = time(&n["occurredAt"]).unwrap_or_else(Local::now);
            let state = s(&n["pullRequestReview"]["state"]);
            let r = reviews.entry(url.clone()).or_insert_with(|| Review {
                repo: s(&pr["repository"]["nameWithOwner"]),
                number: pr["number"].as_u64().unwrap_or(0),
                title: s(&pr["title"]),
                url,
                pr_author: pr["author"]["login"]
                    .as_str()
                    .unwrap_or("ghost")
                    .to_string(),
                states: vec![],
                last_at: at,
            });
            r.states.push(state);
            if at > r.last_at {
                r.last_at = at;
            }
        }
        act.reviews = reviews.into_values().collect();
        act.reviews
            .sort_by(|a, b| a.repo.cmp(&b.repo).then(a.number.cmp(&b.number)));

        // ---- Issues opened ----
        progress("Fetching issues opened…".into());
        let nodes = self.paginate_collection(
            user,
            &from,
            &to,
            "issueContributions",
            "issue { number title url state createdAt repository { nameWithOwner } }",
        )?;
        for n in nodes {
            let i = &n["issue"];
            act.issues.push(Issue {
                repo: s(&i["repository"]["nameWithOwner"]),
                number: i["number"].as_u64().unwrap_or(0),
                title: s(&i["title"]),
                url: s(&i["url"]),
                state: s(&i["state"]).to_lowercase(),
                created_at: time(&i["createdAt"]).unwrap_or_else(Local::now),
            });
        }
        act.issues
            .sort_by(|a, b| a.repo.cmp(&b.repo).then(a.number.cmp(&b.number)));

        // ---- Conversation comments ----
        progress("Searching threads you commented on…".into());
        let q = format!("commenter:{user} updated:>={}", period.from);
        let candidates = self.paginate_search(
            &q,
            "... on Issue { number title url repository { nameWithOwner } }
             ... on PullRequest { number title url repository { nameWithOwner } }",
            MAX_DISCUSSION_THREADS,
        )?;
        if candidates.len() >= MAX_DISCUSSION_THREADS {
            act.warnings.push(format!(
                "Comment search capped at {MAX_DISCUSSION_THREADS} threads; some discussions may be missing."
            ));
        }
        act.discussions = self.verify_comments(
            user,
            &period,
            &from,
            candidates,
            progress,
            &mut act.warnings,
        );
        act.discussions
            .sort_by(|a, b| a.repo.cmp(&b.repo).then(a.number.cmp(&b.number)));

        Ok(act)
    }

    fn paginate_collection(
        &self,
        user: &str,
        from: &str,
        to: &str,
        conn: &str,
        node: &str,
    ) -> Result<Vec<Value>> {
        let query = format!(
            r#"query($login:String!,$from:DateTime!,$to:DateTime!,$after:String){{
              user(login:$login){{ contributionsCollection(from:$from,to:$to){{
                {conn}(first:100, after:$after){{ pageInfo{{ hasNextPage endCursor }} nodes{{ {node} }} }} }} }} }}"#
        );
        let mut out = vec![];
        let mut after = Value::Null;
        loop {
            let d = self.graphql(
                &query,
                json!({ "login": user, "from": from, "to": to, "after": after }),
            )?;
            let c = &d["user"]["contributionsCollection"][conn];
            out.extend(arr(&c["nodes"]).iter().cloned());
            if c["pageInfo"]["hasNextPage"].as_bool() != Some(true) {
                break;
            }
            after = c["pageInfo"]["endCursor"].clone();
        }
        Ok(out)
    }

    fn paginate_search(&self, q: &str, node: &str, limit: usize) -> Result<Vec<Value>> {
        let query = format!(
            r#"query($q:String!,$after:String){{
              search(query:$q, type:ISSUE, first:100, after:$after){{
                pageInfo{{ hasNextPage endCursor }} nodes{{ {node} }} }} }}"#
        );
        let mut out = vec![];
        let mut after = Value::Null;
        loop {
            let d = self.graphql(&query, json!({ "q": q, "after": after }))?;
            let c = &d["search"];
            out.extend(
                arr(&c["nodes"])
                    .iter()
                    .filter(|n| n["url"].is_string())
                    .cloned(),
            );
            if out.len() >= limit || c["pageInfo"]["hasNextPage"].as_bool() != Some(true) {
                break;
            }
            after = c["pageInfo"]["endCursor"].clone();
        }
        out.truncate(limit);
        Ok(out)
    }

    /// For every candidate thread, count the user's conversation comments inside the period.
    fn verify_comments(
        &self,
        user: &str,
        period: &Period,
        since: &str,
        candidates: Vec<Value>,
        progress: &(dyn Fn(String) + Sync),
        warnings: &mut Vec<String>,
    ) -> Vec<Discussion> {
        let total = candidates.len();
        let results = Mutex::new(Vec::new());
        let failures = Mutex::new(0usize);
        let done = Mutex::new(0usize);
        let chunk = total.div_ceil(COMMENT_WORKERS).max(1);

        std::thread::scope(|scope| {
            for part in candidates.chunks(chunk) {
                let (results, failures, done) = (&results, &failures, &done);
                scope.spawn(move || {
                    for n in part {
                        let url = s(&n["url"]);
                        let repo = s(&n["repository"]["nameWithOwner"]);
                        let number = n["number"].as_u64().unwrap_or(0);
                        match self.count_comments(&repo, number, user, period, since) {
                            Ok((count, last)) if count > 0 => {
                                results.lock().unwrap().push(Discussion {
                                    is_pr: url.contains("/pull/"),
                                    repo,
                                    number,
                                    title: s(&n["title"]),
                                    url,
                                    comments: count,
                                    last_at: last.unwrap_or_else(Local::now),
                                });
                            }
                            Ok(_) => {}
                            Err(_) => *failures.lock().unwrap() += 1,
                        }
                        let mut d = done.lock().unwrap();
                        *d += 1;
                        progress(format!("Checking comments… {}/{total}", *d));
                    }
                });
            }
        });

        let failed = failures.into_inner().unwrap();
        if failed > 0 {
            warnings.push(format!("Could not read comments on {failed} thread(s)."));
        }
        results.into_inner().unwrap()
    }

    fn count_comments(
        &self,
        repo: &str,
        number: u64,
        user: &str,
        period: &Period,
        since: &str,
    ) -> Result<(usize, Option<DateTime<Local>>)> {
        let mut count = 0;
        let mut last = None;
        for page in 1..=10 {
            let v = self.rest(&format!(
                "/repos/{repo}/issues/{number}/comments?per_page=100&page={page}&since={}",
                urlencode(since)
            ))?;
            let items = arr(&v);
            for c in items {
                let author = c["user"]["login"].as_str().unwrap_or("");
                if !author.eq_ignore_ascii_case(user) {
                    continue;
                }
                if let Some(t) = time(&c["created_at"])
                    && period.contains(&t)
                {
                    count += 1;
                    if last.is_none_or(|l| t > l) {
                        last = Some(t);
                    }
                }
            }
            if items.len() < 100 {
                break;
            }
        }
        Ok((count, last))
    }
}

const PR_FIELDS: &str = "number title url state isDraft createdAt mergedAt additions deletions repository { nameWithOwner }";

fn parse_pr(v: &Value, period: &Period) -> Option<PullRequest> {
    let url = v["url"].as_str()?.to_string();
    let created_at = time(&v["createdAt"])?;
    let merged_at = time(&v["mergedAt"]);
    let state = match (v["state"].as_str(), v["isDraft"].as_bool()) {
        (Some("MERGED"), _) => PrState::Merged,
        (Some("CLOSED"), _) => PrState::Closed,
        (_, Some(true)) => PrState::Draft,
        _ => PrState::Open,
    };
    Some(PullRequest {
        repo: s(&v["repository"]["nameWithOwner"]),
        number: v["number"].as_u64()?,
        title: s(&v["title"]),
        url,
        state,
        opened_in_period: period.contains(&created_at),
        merged_in_period: merged_at.as_ref().is_some_and(|m| period.contains(m)),
        created_at,
        merged_at,
        additions: v["additions"].as_u64().unwrap_or(0),
        deletions: v["deletions"].as_u64().unwrap_or(0),
    })
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn arr(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn time(v: &Value) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(v.as_str()?)
        .ok()
        .map(|t| t.with_timezone(&Local))
}

fn urlencode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}
