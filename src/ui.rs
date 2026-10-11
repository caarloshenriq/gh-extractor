use crate::app::{App, Field, Screen, Tab};
use crate::model::*;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, Tabs, Wrap,
    },
};

const ACCENT: Color = Color::Cyan;
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn draw(f: &mut Frame, app: &mut App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(f.area());

    draw_header(f, app, header);
    match app.screen {
        Screen::Setup => draw_setup(f, app, body),
        Screen::Loading => draw_loading(f, app, body),
        Screen::Repos => draw_repos(f, app, body),
        Screen::Activity => draw_activity(f, app, body),
    }
    draw_footer(f, app, footer);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![Span::styled(
        " BOSS Report ",
        Style::new().fg(Color::Black).bg(ACCENT).bold(),
    )];
    if matches!(app.screen, Screen::Repos | Screen::Activity) {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("@{}", app.user.trim()),
            Style::new().fg(ACCENT),
        ));
        spans.push(Span::raw("  ·  "));
        spans.push(Span::raw(app.period.label()));
    }
    f.render_widget(Line::from(spans), area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    if let Some(msg) = &app.flash {
        f.render_widget(
            Line::from(Span::styled(
                format!(" {msg}"),
                Style::new().fg(Color::Yellow),
            )),
            area,
        );
        return;
    }
    let keys: &[(&str, &str)] = match app.screen {
        Screen::Setup => &[
            ("Tab/↑↓", "field"),
            ("←→", "month"),
            ("Enter", "fetch"),
            ("Esc", "quit"),
        ],
        Screen::Loading => &[("Esc", "cancel")],
        Screen::Repos => &[
            ("↑↓", "move"),
            ("Space", "toggle"),
            ("a/n/i", "all/none/invert"),
            ("Enter", "continue"),
            ("Esc", "back"),
            ("q", "quit"),
        ],
        Screen::Activity => &[
            ("←→/1-5", "tab"),
            ("↑↓", "move"),
            ("Enter", "open"),
            ("e", "export .md"),
            ("Esc", "back"),
            ("q", "quit"),
        ],
    };
    let mut spans = vec![Span::raw(" ")];
    for (k, d) in keys {
        spans.push(Span::styled(*k, Style::new().fg(ACCENT).bold()));
        spans.push(Span::styled(
            format!(" {d}   "),
            Style::new().fg(Color::DarkGray),
        ));
    }
    f.render_widget(Line::from(spans), area);
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

fn panel(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(Color::DarkGray))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(ACCENT).bold(),
        ))
}

// ---------------- Setup ----------------

fn draw_setup(f: &mut Frame, app: &App, area: Rect) {
    let box_area = centered(area, 64, 17);
    f.render_widget(Clear, box_area);
    let block = panel("Monthly contribution report");
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);

    let rows = Layout::vertical([
        Constraint::Length(2), // intro
        Constraint::Length(1), // user
        Constraint::Length(1),
        Constraint::Length(1), // month
        Constraint::Length(1),
        Constraint::Length(1), // from
        Constraint::Length(1), // to
        Constraint::Length(1),
        Constraint::Length(1), // period
        Constraint::Min(0),    // messages
    ])
    .split(inner.inner(ratatui::layout::Margin::new(2, 1)));

    f.render_widget(
        Paragraph::new("Pick who and when. Contributions are fetched from GitHub.")
            .fg(Color::DarkGray),
        rows[0],
    );

    let label_w = 14u16;
    let field = |f: &mut Frame, area: Rect, label: &str, value: Line, focused: bool| {
        let [l, v] =
            Layout::horizontal([Constraint::Length(label_w), Constraint::Min(0)]).areas(area);
        let ls = if focused {
            Style::new().fg(ACCENT).bold()
        } else {
            Style::new().fg(Color::Gray)
        };
        let marker = if focused { "› " } else { "  " };
        f.render_widget(Span::styled(format!("{marker}{label}"), ls), l);
        f.render_widget(value, v);
    };
    let input_style = |focused: bool| {
        if focused {
            Style::new().fg(Color::White).bg(Color::Rgb(40, 44, 52))
        } else {
            Style::new().fg(Color::White)
        }
    };

    // User
    let focused = app.focus == Field::User;
    let text = if app.user.is_empty() && !focused {
        "(type a username)".to_string()
    } else {
        app.user.clone()
    };
    field(
        f,
        rows[1],
        "GitHub user",
        Line::from(Span::styled(format!(" {text:<30}"), input_style(focused))),
        focused,
    );
    if focused {
        f.set_cursor_position(Position::new(
            rows[1].x + label_w + 1 + app.user.len() as u16,
            rows[1].y,
        ));
    }

    // Month
    let focused = app.focus == Field::Month;
    let month = Period::month(app.month.0, app.month.1).label();
    let arrow = if focused {
        Style::new().fg(ACCENT).bold()
    } else {
        Style::new().fg(Color::DarkGray)
    };
    field(
        f,
        rows[3],
        "Month",
        Line::from(vec![
            Span::styled("◀ ", arrow),
            Span::styled(
                format!("{month:^16}"),
                input_style(focused).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ▶", arrow),
        ]),
        focused,
    );

    // From / To
    for (row, fld, label, val) in [
        (rows[5], Field::From, "From", &app.from_input),
        (rows[6], Field::To, "To", &app.to_input),
    ] {
        let focused = app.focus == fld;
        field(
            f,
            row,
            label,
            Line::from(Span::styled(format!(" {val:<12}"), input_style(focused))),
            focused,
        );
        if focused {
            f.set_cursor_position(Position::new(row.x + label_w + 1 + val.len() as u16, row.y));
        }
    }

    field(
        f,
        rows[8],
        "Period",
        Line::from(Span::styled(
            app.period_hint(),
            Style::new().fg(Color::Green),
        )),
        false,
    );

    let mut msgs = vec![];
    if let Some(e) = &app.error {
        msgs.push(Line::from(Span::styled(
            format!("✗ {e}"),
            Style::new().fg(Color::Red),
        )));
    }
    if let Some(n) = &app.notice {
        msgs.push(Line::from(Span::styled(
            n.clone(),
            Style::new().fg(Color::Yellow),
        )));
    }
    f.render_widget(Paragraph::new(msgs).wrap(Wrap { trim: true }), rows[9]);
}

// ---------------- Loading ----------------

fn draw_loading(f: &mut Frame, app: &App, area: Rect) {
    let box_area = centered(area, 60, 7);
    let block = panel("Fetching from GitHub");
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);
    let spin = SPINNER[app.tick % SPINNER.len()];
    let text = vec![
        Line::raw(""),
        Line::from(vec![
            Span::styled(format!("{spin} "), Style::new().fg(ACCENT)),
            Span::raw(app.status.clone()),
        ]),
        Line::raw(""),
        Line::from(Span::styled(
            format!("@{} · {}", app.user.trim(), app.period.label()),
            Style::new().fg(Color::DarkGray),
        )),
    ];
    f.render_widget(Paragraph::new(text).centered(), inner);
}

// ---------------- Repos ----------------

fn plural(n: impl Into<u64>, one: &str, many: &str) -> String {
    let n = n.into();
    format!("{n} {}", if n == 1 { one } else { many })
}

fn repo_summary(r: &RepoStats) -> Vec<Span<'static>> {
    let mut parts = vec![];
    let mut push = |n: u64, one: &str, many: &str, c: Color| {
        if n > 0 {
            if !parts.is_empty() {
                parts.push(Span::styled(" · ", Style::new().fg(Color::DarkGray)));
            }
            parts.push(Span::styled(plural(n, one, many), Style::new().fg(c)));
        }
    };
    push(r.prs as u64, "PR", "PRs", Color::Magenta);
    push(r.reviews as u64, "review", "reviews", Color::Blue);
    push(r.issues as u64, "issue", "issues", Color::Green);
    push(
        r.discussions as u64,
        "discussion",
        "discussions",
        Color::Yellow,
    );
    push(r.commits, "commit", "commits", Color::Gray);
    parts
}

fn draw_repos(f: &mut Frame, app: &mut App, area: Rect) {
    let title = format!(
        "Repositories you contributed to  ({}/{} selected)",
        app.selected_count(),
        app.repos.len()
    );
    let block = panel(&title);

    if app.repos.is_empty() {
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new(vec![
                Line::raw(""),
                Line::raw("No public contributions found for this period."),
                Line::from(Span::styled(
                    "Press Esc to pick another user or period.",
                    Style::new().fg(Color::DarkGray),
                )),
            ])
            .centered(),
            inner,
        );
        return;
    }

    let name_w = app
        .repos
        .iter()
        .map(|r| r.name.len())
        .max()
        .unwrap_or(10)
        .min(48);
    let items: Vec<ListItem> = app
        .repos
        .iter()
        .zip(&app.checked)
        .map(|(r, checked)| {
            let (mark, ms) = if *checked {
                ("[✓] ", Style::new().fg(Color::Green).bold())
            } else {
                ("[ ] ", Style::new().fg(Color::DarkGray))
            };
            let mut spans = vec![
                Span::styled(mark, ms),
                Span::styled(
                    format!("{:<name_w$}  ", truncate(&r.name, name_w)),
                    Style::new().bold(),
                ),
            ];
            spans.extend(repo_summary(r));
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().bg(Color::Rgb(40, 44, 52)))
        .highlight_symbol("› ");
    f.render_stateful_widget(list, area, &mut app.repo_state);
}

// ---------------- Activity ----------------

fn draw_activity(f: &mut Frame, app: &mut App, area: Rect) {
    let [tabs_area, body] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(area);

    let titles: Vec<Line> = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let count = match t {
                Tab::Summary => String::new(),
                _ => format!(" ({})", app.tab_len(*t)),
            };
            Line::from(format!("{} {}{count}", i + 1, t.title()))
        })
        .collect();
    let selected = Tab::ALL.iter().position(|t| *t == app.tab).unwrap_or(0);
    let tabs = Tabs::new(titles)
        .select(selected)
        .style(Style::new().fg(Color::Gray))
        .highlight_style(Style::new().fg(ACCENT).bold().underlined())
        .divider(Span::styled("│", Style::new().fg(Color::DarkGray)))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::new().fg(Color::DarkGray)),
        );
    f.render_widget(tabs, tabs_area);

    match app.tab {
        Tab::Summary => draw_summary(f, app, body),
        Tab::PullRequests => draw_prs(f, app, body),
        Tab::Reviews => draw_reviews(f, app, body),
        Tab::Issues => draw_issues(f, app, body),
        Tab::Discussions => draw_discussions(f, app, body),
    }
}

fn header(cols: &[&'static str]) -> Row<'static> {
    Row::new(cols.iter().map(|c| Cell::from(*c)))
        .style(Style::new().fg(ACCENT).bold())
        .bottom_margin(0)
}

fn table<'a>(rows: Vec<Row<'a>>, widths: Vec<Constraint>, head: Row<'a>) -> Table<'a> {
    Table::new(rows, widths)
        .header(head)
        .column_spacing(2)
        .row_highlight_style(Style::new().bg(Color::Rgb(40, 44, 52)))
        .highlight_symbol("› ")
}

fn empty(f: &mut Frame, area: Rect, what: &str) {
    f.render_widget(
        Paragraph::new(vec![
            Line::raw(""),
            Line::from(Span::styled(
                format!("No {what} in this period."),
                Style::new().fg(Color::DarkGray),
            )),
        ])
        .centered(),
        area,
    );
}

fn repo_w(names: impl Iterator<Item = usize>) -> u16 {
    names.max().unwrap_or(10).clamp(10, 36) as u16
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

fn state_style(s: PrState) -> Style {
    match s {
        PrState::Merged => Style::new().fg(Color::Magenta),
        PrState::Open => Style::new().fg(Color::Green),
        PrState::Draft => Style::new().fg(Color::DarkGray),
        PrState::Closed => Style::new().fg(Color::Red),
    }
}

fn draw_summary(f: &mut Frame, app: &mut App, area: Rect) {
    let v = &app.view;
    let repos = v.repos();
    let merged = v
        .pull_requests
        .iter()
        .filter(|p| p.merged_in_period)
        .count();
    let opened = v
        .pull_requests
        .iter()
        .filter(|p| p.opened_in_period)
        .count();
    let commits: u64 = v.commits.values().sum();
    let comments: usize = v.discussions.iter().map(|d| d.comments).sum();
    let (adds, dels) = v
        .pull_requests
        .iter()
        .filter(|p| p.merged_in_period)
        .fold((0, 0), |(a, d), p| (a + p.additions, d + p.deletions));

    let warn_h = if v.warnings.is_empty() {
        0
    } else {
        v.warnings.len() as u16 + 1
    };
    let [cards, tbl, warn] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(0),
        Constraint::Length(warn_h),
    ])
    .areas(area);

    // Stat cards
    let stats = [
        ("PRs opened", opened.to_string(), Color::Magenta),
        ("PRs merged", merged.to_string(), Color::Magenta),
        ("Reviewed", v.reviews.len().to_string(), Color::Blue),
        ("Issues", v.issues.len().to_string(), Color::Green),
        ("Comments", comments.to_string(), Color::Yellow),
        ("Commits", commits.to_string(), Color::Gray),
    ];
    let cols = Layout::horizontal(
        stats
            .iter()
            .map(|_| Constraint::Ratio(1, stats.len() as u32)),
    )
    .split(cards);
    for ((label, val, color), c) in stats.iter().zip(cols.iter()) {
        let b = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::DarkGray));
        let inner = b.inner(*c);
        f.render_widget(b, *c);
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(val.clone(), Style::new().fg(*color).bold())),
                Line::from(Span::styled(*label, Style::new().fg(Color::DarkGray))),
            ])
            .centered(),
            inner,
        );
    }

    let rw = repo_w(repos.iter().map(|r| r.name.len()));
    let rows: Vec<Row> = repos
        .iter()
        .map(|r| {
            let merged = v
                .pull_requests
                .iter()
                .filter(|p| p.repo == r.name && p.merged_in_period)
                .count();
            let comments: usize = v
                .discussions
                .iter()
                .filter(|d| d.repo == r.name)
                .map(|d| d.comments)
                .sum();
            Row::new(vec![
                Cell::from(truncate(&r.name, rw as usize)).bold(),
                Cell::from(r.prs.to_string()),
                Cell::from(merged.to_string()),
                Cell::from(r.reviews.to_string()),
                Cell::from(r.issues.to_string()),
                Cell::from(comments.to_string()),
                Cell::from(r.commits.to_string()),
            ])
        })
        .collect();
    let widths = vec![
        Constraint::Length(rw),
        Constraint::Length(5),
        Constraint::Length(7),
        Constraint::Length(8),
        Constraint::Length(7),
        Constraint::Length(9),
        Constraint::Length(8),
    ];
    let t = table(
        rows,
        widths,
        header(&[
            "Repository",
            "PRs",
            "Merged",
            "Reviews",
            "Issues",
            "Comments",
            "Commits",
        ]),
    )
    .block(panel(&format!(
        "By repository · merged diff +{adds}/−{dels}"
    )));
    f.render_stateful_widget(t, tbl, &mut app.table_state);

    if !v.warnings.is_empty() {
        let lines: Vec<Line> = v
            .warnings
            .iter()
            .map(|w| {
                Line::from(Span::styled(
                    format!(" ⚠ {w}"),
                    Style::new().fg(Color::Yellow),
                ))
            })
            .collect();
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), warn);
    }
}

fn draw_prs(f: &mut Frame, app: &mut App, area: Rect) {
    let v = &app.view;
    if v.pull_requests.is_empty() {
        return empty(f, area, "pull requests");
    }
    let rw = repo_w(v.pull_requests.iter().map(|p| p.repo.len()));
    let rows: Vec<Row> = v
        .pull_requests
        .iter()
        .map(|p| {
            let note = if p.merged_in_period && !p.opened_in_period {
                " *"
            } else {
                ""
            };
            Row::new(vec![
                Cell::from(truncate(&p.repo, rw as usize)).fg(Color::Gray),
                Cell::from(format!("#{}", p.number)).fg(Color::DarkGray),
                Cell::from(p.title.clone()),
                Cell::from(format!("{}{note}", p.state.label())).style(state_style(p.state)),
                Cell::from(p.created_at.format("%Y-%m-%d").to_string()).fg(Color::DarkGray),
                Cell::from(
                    p.merged_at
                        .map(|m| m.format("%Y-%m-%d").to_string())
                        .unwrap_or_else(|| "—".into()),
                )
                .fg(Color::DarkGray),
                Cell::from(Line::from(vec![
                    Span::styled(format!("+{}", p.additions), Style::new().fg(Color::Green)),
                    Span::raw(" "),
                    Span::styled(format!("−{}", p.deletions), Style::new().fg(Color::Red)),
                ])),
            ])
        })
        .collect();
    let widths = vec![
        Constraint::Length(rw),
        Constraint::Length(6),
        Constraint::Min(20),
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(13),
    ];
    let t = table(
        rows,
        widths,
        header(&[
            "Repository",
            "#",
            "Title",
            "State",
            "Opened",
            "Merged",
            "Diff",
        ]),
    )
    .block(panel(
        "Pull requests  (* merged this period, opened earlier)",
    ));
    f.render_stateful_widget(t, area, &mut app.table_state);
}

fn draw_reviews(f: &mut Frame, app: &mut App, area: Rect) {
    let v = &app.view;
    if v.reviews.is_empty() {
        return empty(f, area, "reviews");
    }
    let rw = repo_w(v.reviews.iter().map(|p| p.repo.len()));
    let rows: Vec<Row> = v
        .reviews
        .iter()
        .map(|r| {
            let color = if r.states.iter().any(|s| s == "APPROVED") {
                Color::Green
            } else if r.states.iter().any(|s| s == "CHANGES_REQUESTED") {
                Color::Red
            } else {
                Color::Blue
            };
            Row::new(vec![
                Cell::from(truncate(&r.repo, rw as usize)).fg(Color::Gray),
                Cell::from(format!("#{}", r.number)).fg(Color::DarkGray),
                Cell::from(r.title.clone()),
                Cell::from(format!("@{}", r.pr_author)).fg(Color::Gray),
                Cell::from(r.summary()).fg(color),
                Cell::from(r.last_at.format("%Y-%m-%d").to_string()).fg(Color::DarkGray),
            ])
        })
        .collect();
    let widths = vec![
        Constraint::Length(rw),
        Constraint::Length(6),
        Constraint::Min(20),
        Constraint::Length(16),
        Constraint::Length(38),
        Constraint::Length(10),
    ];
    let t = table(
        rows,
        widths,
        header(&["Repository", "#", "Title", "Author", "Review", "Last"]),
    )
    .block(panel("Pull requests reviewed"));
    f.render_stateful_widget(t, area, &mut app.table_state);
}

fn draw_issues(f: &mut Frame, app: &mut App, area: Rect) {
    let v = &app.view;
    if v.issues.is_empty() {
        return empty(f, area, "issues opened");
    }
    let rw = repo_w(v.issues.iter().map(|p| p.repo.len()));
    let rows: Vec<Row> = v
        .issues
        .iter()
        .map(|i| {
            let c = if i.state == "open" {
                Color::Green
            } else {
                Color::Magenta
            };
            Row::new(vec![
                Cell::from(truncate(&i.repo, rw as usize)).fg(Color::Gray),
                Cell::from(format!("#{}", i.number)).fg(Color::DarkGray),
                Cell::from(i.title.clone()),
                Cell::from(i.state.clone()).fg(c),
                Cell::from(i.created_at.format("%Y-%m-%d").to_string()).fg(Color::DarkGray),
            ])
        })
        .collect();
    let widths = vec![
        Constraint::Length(rw),
        Constraint::Length(6),
        Constraint::Min(20),
        Constraint::Length(8),
        Constraint::Length(10),
    ];
    let t = table(
        rows,
        widths,
        header(&["Repository", "#", "Title", "State", "Opened"]),
    )
    .block(panel("Issues opened"));
    f.render_stateful_widget(t, area, &mut app.table_state);
}

fn draw_discussions(f: &mut Frame, app: &mut App, area: Rect) {
    let v = &app.view;
    if v.discussions.is_empty() {
        return empty(f, area, "comments");
    }
    let rw = repo_w(v.discussions.iter().map(|p| p.repo.len()));
    let rows: Vec<Row> = v
        .discussions
        .iter()
        .map(|d| {
            Row::new(vec![
                Cell::from(truncate(&d.repo, rw as usize)).fg(Color::Gray),
                Cell::from(format!("#{}", d.number)).fg(Color::DarkGray),
                Cell::from(d.title.clone()),
                Cell::from(if d.is_pr { "PR" } else { "issue" }).fg(Color::Gray),
                Cell::from(d.comments.to_string()).fg(Color::Yellow),
                Cell::from(d.last_at.format("%Y-%m-%d").to_string()).fg(Color::DarkGray),
            ])
        })
        .collect();
    let widths = vec![
        Constraint::Length(rw),
        Constraint::Length(6),
        Constraint::Min(20),
        Constraint::Length(5),
        Constraint::Length(8),
        Constraint::Length(10),
    ];
    let t = table(
        rows,
        widths,
        header(&["Repository", "#", "Title", "Type", "Comments", "Last"]),
    )
    .block(panel(
        "Threads commented on (conversation comments, excluding reviews)",
    ));
    f.render_stateful_widget(t, area, &mut app.table_state);
}
