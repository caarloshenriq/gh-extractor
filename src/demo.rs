//! Sample data for `--demo`, so the UI can be tried without a token.

use crate::model::*;
use chrono::{Local, NaiveTime, TimeZone};

pub fn activity(user: &str, period: &Period) -> Activity {
    let at = |day: u32| {
        let d = period.from + chrono::Days::new(u64::from(day.saturating_sub(1)));
        let d = d.min(period.to);
        Local
            .from_local_datetime(&d.and_time(NaiveTime::from_hms_opt(14, 0, 0).unwrap()))
            .earliest()
            .unwrap()
    };
    let pr = |repo: &str,
              n: u64,
              title: &str,
              state: PrState,
              day: u32,
              merged: bool,
              add: u64,
              del: u64| PullRequest {
        repo: repo.into(),
        number: n,
        title: title.into(),
        url: format!("https://github.com/{repo}/pull/{n}"),
        state,
        created_at: at(day),
        merged_at: merged.then(|| at(day + 3)),
        opened_in_period: true,
        merged_in_period: merged,
        additions: add,
        deletions: del,
    };
    let review =
        |repo: &str, n: u64, title: &str, author: &str, states: &[&str], day: u32| Review {
            repo: repo.into(),
            number: n,
            title: title.into(),
            url: format!("https://github.com/{repo}/pull/{n}"),
            pr_author: author.into(),
            states: states.iter().map(|s| s.to_string()).collect(),
            last_at: at(day),
        };

    let mut older = pr(
        "payjoin/rust-payjoin",
        801,
        "Add fuzz target for v2 session persistence",
        PrState::Merged,
        1,
        true,
        312,
        18,
    );
    older.opened_in_period = false;
    older.created_at = at(1) - chrono::Days::new(20);

    Activity {
        user: user.into(),
        pull_requests: vec![
            older,
            pr(
                "payjoin/rust-payjoin",
                842,
                "no_std: gate receiver behind alloc feature",
                PrState::Merged,
                4,
                true,
                540,
                211,
            ),
            pr(
                "payjoin/rust-payjoin",
                857,
                "ffi: expose persister callbacks to Python bindings",
                PrState::Open,
                12,
                false,
                188,
                40,
            ),
            pr(
                "payjoin/rust-payjoin",
                861,
                "Fix off-by-one in OHTTP key rotation check",
                PrState::Draft,
                20,
                false,
                12,
                3,
            ),
            pr(
                "bitcoindevkit/bdk",
                2011,
                "docs: clarify Wallet::sign finalization options",
                PrState::Merged,
                8,
                true,
                45,
                9,
            ),
        ],
        reviews: vec![
            review(
                "payjoin/rust-payjoin",
                838,
                "Introduce directory health endpoint",
                "spacebear21",
                &["COMMENTED", "APPROVED"],
                3,
            ),
            review(
                "payjoin/rust-payjoin",
                850,
                "Refactor sender state machine",
                "DanGould",
                &["CHANGES_REQUESTED", "COMMENTED", "APPROVED"],
                15,
            ),
            review(
                "payjoin/rust-payjoin",
                866,
                "Bump MSRV to 1.85",
                "nothingmuch",
                &["APPROVED"],
                25,
            ),
            review(
                "bitcoindevkit/bdk",
                2005,
                "chain: add CanonicalView helper",
                "evanlinjin",
                &["COMMENTED"],
                18,
            ),
        ],
        issues: vec![
            Issue {
                repo: "payjoin/rust-payjoin".into(),
                number: 855,
                title: "Receiver panics on malformed Original PSBT with zero inputs".into(),
                url: "https://github.com/payjoin/rust-payjoin/issues/855".into(),
                state: "closed".into(),
                created_at: at(10),
            },
            Issue {
                repo: "curitibabitdevs/curitibabitdevs.org".into(),
                number: 14,
                title: "Add October meetup page".into(),
                url: "https://github.com/curitibabitdevs/curitibabitdevs.org/issues/14".into(),
                state: "open".into(),
                created_at: at(22),
            },
        ],
        discussions: vec![
            Discussion {
                repo: "payjoin/rust-payjoin".into(),
                number: 790,
                title: "Tracking: embedded / no_std support".into(),
                url: "https://github.com/payjoin/rust-payjoin/issues/790".into(),
                is_pr: false,
                comments: 3,
                last_at: at(9),
            },
            Discussion {
                repo: "bitcoindevkit/bdk".into(),
                number: 1998,
                title: "RFC: wallet persistence changeset format".into(),
                url: "https://github.com/bitcoindevkit/bdk/issues/1998".into(),
                is_pr: false,
                comments: 1,
                last_at: at(17),
            },
        ],
        commits: [
            ("payjoin/rust-payjoin".to_string(), 17),
            ("bitcoindevkit/bdk".to_string(), 2),
        ]
        .into(),
        warnings: vec![],
    }
}
