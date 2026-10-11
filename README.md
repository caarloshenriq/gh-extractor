# boss-report

A terminal UI that pulls your GitHub contributions for a period and helps you write the monthly BOSS program report.

```
Setup (user + month) → pick repositories → browse PRs / reviews / issues / discussions → export Markdown
```

## Install

```sh
cargo install --path .
```

Authentication: the app uses `GITHUB_TOKEN`, then `GH_TOKEN`, then `gh auth token`.
If you already use the GitHub CLI, there is nothing to configure. A classic token with
`read:user` + `public_repo` (or a fine-grained token with public read access) is enough.

## Usage

```sh
boss-report                                   # current month, user = token owner
boss-report --month 2026-08                   # a specific month
boss-report -u caarloshenriq --from 2026-09-01 --to 2026-09-15
boss-report --month 2026-09 --print > report.md   # no TUI, all repos, straight to Markdown
boss-report --demo                            # sample data, no network
```

### Keys

| Screen | Keys |
|---|---|
| Setup | `Tab`/`↑↓` move between fields · `←→` change month · type to edit user / dates · `Enter` fetch |
| Repositories | `Space` toggle · `a` all · `n` none · `i` invert · `Enter` continue · `Esc` back |
| Activity | `←→` or `1–5` switch tab · `↑↓` move · `Enter`/`o` open in browser · `e` export `.md` · `Esc` back |

`e` writes `boss-report-<user>-<period>.md` in the current directory, containing only the repositories you selected.

## What is collected

| Section | Source |
|---|---|
| PRs opened | GraphQL `contributionsCollection.pullRequestContributions` |
| PRs merged | Search `is:pr author:<user> merged:<range>` (catches PRs opened in an earlier month) |
| Reviews | `pullRequestReviewContributions`, grouped per PR with each review state |
| Issues opened | `issueContributions` |
| Discussions | Search `commenter:<user>`, then each thread's comments are checked so only comments written inside the period count |
| Commits | `commitContributionsByRepository` (default-branch commits, like the contribution graph) |

Dates are interpreted in your local time zone.

## Limitations

- Private contributions that GitHub hides are only reported as a count.
- The comment search is capped at 200 threads per period.
- Inline review comments are counted as part of the review, not under Discussions.
