//! Markdown export, shaped as a monthly report draft.

use crate::model::*;
use std::fmt::Write;

pub fn markdown(act: &Activity, period: &Period) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "# Contribution report — {}\n", period.label());
    let _ = writeln!(o, "**GitHub:** [@{0}](https://github.com/{0})  ", act.user);
    let _ = writeln!(o, "**Period:** {} to {}\n", period.from, period.to);

    let merged = act
        .pull_requests
        .iter()
        .filter(|p| p.merged_in_period)
        .count();
    let opened = act
        .pull_requests
        .iter()
        .filter(|p| p.opened_in_period)
        .count();
    let commits: u64 = act.commits.values().sum();
    let _ = writeln!(o, "## Summary\n");
    let _ = writeln!(o, "| | Count |\n|---|---:|");
    let _ = writeln!(o, "| Pull requests opened | {opened} |");
    let _ = writeln!(o, "| Pull requests merged | {merged} |");
    let _ = writeln!(o, "| PRs reviewed | {} |", act.reviews.len());
    let _ = writeln!(o, "| Issues opened | {} |", act.issues.len());
    let _ = writeln!(o, "| Threads commented on | {} |", act.discussions.len());
    let _ = writeln!(o, "| Commits (default branches) | {commits} |\n");

    for repo in act.repos() {
        let r = &repo.name;
        let _ = writeln!(o, "## [{r}](https://github.com/{r})\n");

        let prs: Vec<_> = act.pull_requests.iter().filter(|p| &p.repo == r).collect();
        if !prs.is_empty() {
            let _ = writeln!(o, "### Pull requests\n");
            for p in prs {
                let mut tags = vec![p.state.label().to_string()];
                if p.merged_in_period && !p.opened_in_period {
                    tags.push(format!("opened {}", p.created_at.format("%Y-%m-%d")));
                }
                let _ = writeln!(
                    o,
                    "- [#{}]({}) {} — {} (+{}/−{})",
                    p.number,
                    p.url,
                    p.title,
                    tags.join(", "),
                    p.additions,
                    p.deletions
                );
            }
            o.push('\n');
        }

        let reviews: Vec<_> = act.reviews.iter().filter(|x| &x.repo == r).collect();
        if !reviews.is_empty() {
            let _ = writeln!(o, "### Reviews\n");
            for x in reviews {
                let _ = writeln!(
                    o,
                    "- [#{}]({}) {} by @{} — {}",
                    x.number,
                    x.url,
                    x.title,
                    x.pr_author,
                    x.summary()
                );
            }
            o.push('\n');
        }

        let issues: Vec<_> = act.issues.iter().filter(|x| &x.repo == r).collect();
        if !issues.is_empty() {
            let _ = writeln!(o, "### Issues opened\n");
            for x in issues {
                let _ = writeln!(o, "- [#{}]({}) {} — {}", x.number, x.url, x.title, x.state);
            }
            o.push('\n');
        }

        let disc: Vec<_> = act.discussions.iter().filter(|x| &x.repo == r).collect();
        if !disc.is_empty() {
            let _ = writeln!(o, "### Discussions\n");
            for x in disc {
                let kind = if x.is_pr { "PR" } else { "issue" };
                let plural = if x.comments == 1 { "" } else { "s" };
                let _ = writeln!(
                    o,
                    "- [#{}]({}) {} ({kind}) — {} comment{plural}",
                    x.number, x.url, x.title, x.comments
                );
            }
            o.push('\n');
        }

        if let Some(n) = act.commits.get(r) {
            let _ = writeln!(o, "_{n} commit(s) to the default branch._\n");
        }
    }

    if !act.warnings.is_empty() {
        let _ = writeln!(o, "---\n");
        for w in &act.warnings {
            let _ = writeln!(o, "> ⚠ {w}");
        }
    }
    o
}
