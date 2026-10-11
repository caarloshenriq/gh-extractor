mod app;
mod demo;
mod github;
mod model;
mod report;
mod ui;

use anyhow::{Result, bail};
use app::{App, Options};
use chrono::NaiveDate;
use clap::Parser;
use model::Period;
use ratatui::crossterm::event::{self, Event};
use std::time::Duration;

/// Browse your GitHub contributions for a period and export a monthly report.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// GitHub username (defaults to the token owner)
    #[arg(short, long)]
    user: Option<String>,

    /// Month to report, as YYYY-MM (defaults to the current month)
    #[arg(short, long, conflicts_with_all = ["from", "to"])]
    month: Option<String>,

    /// Start date, YYYY-MM-DD (use with --to)
    #[arg(long, requires = "to")]
    from: Option<NaiveDate>,

    /// End date, YYYY-MM-DD (use with --from)
    #[arg(long, requires = "from")]
    to: Option<NaiveDate>,

    /// Skip the TUI and print the Markdown report for all repositories to stdout
    #[arg(long)]
    print: bool,

    /// Use sample data instead of calling GitHub
    #[arg(long)]
    demo: bool,
}

fn parse_month(s: &str) -> Result<Period> {
    let d = NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("--month must look like 2026-09"))?;
    use chrono::Datelike;
    Ok(Period::month(d.year(), d.month()))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let period = match (&cli.month, cli.from, cli.to) {
        (Some(m), _, _) => Some(parse_month(m)?),
        (None, Some(from), Some(to)) => Some(Period { from, to }),
        _ => None,
    };
    let token = if cli.demo { None } else { github::find_token() };

    if cli.print {
        return print_report(cli.user, period, token, cli.demo);
    }

    let mut app = App::new(Options {
        user: cli.user,
        period,
        token,
        demo: cli.demo,
    });
    let mut terminal = ratatui::init();
    let result = (|| -> Result<()> {
        while !app.quit {
            app.on_tick();
            terminal.draw(|f| ui::draw(f, &mut app))?;
            if event::poll(Duration::from_millis(80))? {
                if let Event::Key(key) = event::read()? {
                    app.on_key(key);
                }
            }
        }
        Ok(())
    })();
    ratatui::restore();
    result
}

fn print_report(
    user: Option<String>,
    period: Option<Period>,
    token: Option<String>,
    demo: bool,
) -> Result<()> {
    use chrono::{Datelike, Local};
    let today = Local::now().date_naive();
    let period = period.unwrap_or_else(|| Period::month(today.year(), today.month()));
    let act = if demo {
        demo::activity(user.as_deref().unwrap_or("demo-user"), &period)
    } else {
        let Some(token) = token else {
            bail!("No GitHub token found. Set GITHUB_TOKEN or run `gh auth login`.")
        };
        let gh = github::GitHub::new(token)?;
        let user = match user {
            Some(u) => u,
            None => gh.viewer()?,
        };
        gh.fetch(&user, period, &|s| eprintln!("{s}"))?
    };
    print!("{}", report::markdown(&act, &period));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn render(app: &mut App) -> String {
        let mut t = Terminal::new(TestBackend::new(130, 26)).unwrap();
        t.draw(|f| ui::draw(f, app)).unwrap();
        let buf = t.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn walk_through_demo() {
        let mut app = App::new(Options {
            user: Some("caarloshenriq".into()),
            period: Some(Period::month(2026, 9)),
            token: None,
            demo: true,
        });
        let mut out = String::new();
        out += &format!("=== SETUP ===\n{}\n", render(&mut app));
        key(&mut app, KeyCode::Left);
        assert_eq!(app.from_input, "2026-08-01");
        assert_eq!(app.to_input, "2026-08-31");
        key(&mut app, KeyCode::Right);
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.screen, app::Screen::Repos);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Char(' '));
        out += &format!("=== REPOS ===\n{}\n", render(&mut app));
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.screen, app::Screen::Activity);
        for tab in 0..5 {
            out += &format!("=== TAB {tab} ===\n{}\n", render(&mut app));
            key(&mut app, KeyCode::Right);
        }
        std::fs::write(std::env::temp_dir().join("boss-render.txt"), out).unwrap();
    }
}
