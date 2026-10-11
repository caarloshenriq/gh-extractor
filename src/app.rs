use crate::github::GitHub;
use crate::model::*;
use crate::report;
use anyhow::Result;
use chrono::{Datelike, Local, NaiveDate};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::widgets::{ListState, TableState};
use std::sync::mpsc::{Receiver, Sender, channel};

pub enum Msg {
    Viewer(String),
    Progress(String),
    Done(Result<Activity>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Setup,
    Loading,
    Repos,
    Activity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    User,
    Month,
    From,
    To,
}

impl Field {
    const ALL: [Field; 4] = [Field::User, Field::Month, Field::From, Field::To];
    fn step(self, d: isize) -> Field {
        let i = Self::ALL.iter().position(|f| *f == self).unwrap() as isize;
        Self::ALL[(i + d).rem_euclid(Self::ALL.len() as isize) as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Summary,
    PullRequests,
    Reviews,
    Issues,
    Discussions,
}

impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::Summary,
        Tab::PullRequests,
        Tab::Reviews,
        Tab::Issues,
        Tab::Discussions,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Tab::Summary => "Summary",
            Tab::PullRequests => "Pull Requests",
            Tab::Reviews => "Reviews",
            Tab::Issues => "Issues",
            Tab::Discussions => "Discussions",
        }
    }
    fn step(self, d: isize) -> Tab {
        let i = Self::ALL.iter().position(|t| *t == self).unwrap() as isize;
        Self::ALL[(i + d).rem_euclid(Self::ALL.len() as isize) as usize]
    }
}

pub struct App {
    pub screen: Screen,
    pub quit: bool,
    token: Option<String>,
    demo: bool,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,

    // Setup
    pub user: String,
    pub month: (i32, u32),
    pub from_input: String,
    pub to_input: String,
    pub focus: Field,
    pub error: Option<String>,
    pub notice: Option<String>,

    // Loading
    pub status: String,
    pub tick: usize,

    // Repos
    pub period: Period,
    pub activity: Option<Activity>,
    pub repos: Vec<RepoStats>,
    pub checked: Vec<bool>,
    pub repo_state: ListState,

    // Activity
    pub view: Activity,
    pub tab: Tab,
    pub table_state: TableState,
    pub flash: Option<String>,
}

pub struct Options {
    pub user: Option<String>,
    pub period: Option<Period>,
    pub token: Option<String>,
    pub demo: bool,
}

impl App {
    pub fn new(opts: Options) -> Self {
        let (tx, rx) = channel();
        let today = Local::now().date_naive();
        let period = opts
            .period
            .unwrap_or_else(|| Period::month(today.year(), today.month()));
        let mut app = App {
            screen: Screen::Setup,
            quit: false,
            token: opts.token,
            demo: opts.demo,
            tx,
            rx,
            user: opts.user.clone().unwrap_or_default(),
            month: (period.from.year(), period.from.month()),
            from_input: period.from.to_string(),
            to_input: period.to.to_string(),
            focus: Field::User,
            error: None,
            notice: None,
            status: String::new(),
            tick: 0,
            period,
            activity: None,
            repos: vec![],
            checked: vec![],
            repo_state: ListState::default(),
            view: Activity::default(),
            tab: Tab::Summary,
            table_state: TableState::default(),
            flash: None,
        };
        if app.demo {
            if app.user.is_empty() {
                app.user = "demo-user".into();
            }
            app.notice = Some("Demo mode: showing sample data, GitHub is not contacted.".into());
        } else if app.token.is_none() {
            app.error =
                Some("No GitHub token found. Set GITHUB_TOKEN or run `gh auth login`.".into());
        } else if app.user.is_empty() {
            // Pre-fill the user with the token owner.
            let (tx, token) = (app.tx.clone(), app.token.clone().unwrap());
            std::thread::spawn(move || {
                if let Ok(login) = GitHub::new(token).and_then(|g| g.viewer()) {
                    let _ = tx.send(Msg::Viewer(login));
                }
            });
        }
        if !app.user.is_empty() {
            app.focus = Field::Month;
        }
        app
    }

    /// Called every frame: advances the spinner and drains background messages.
    pub fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Viewer(login) => {
                    if self.user.is_empty() {
                        self.user = login;
                        if self.focus == Field::User {
                            self.focus = Field::Month;
                        }
                    }
                }
                Msg::Progress(s) => self.status = s,
                Msg::Done(Ok(act)) => self.load_activity(act),
                Msg::Done(Err(e)) => {
                    self.error = Some(format!("{e:#}"));
                    self.screen = Screen::Setup;
                }
            }
        }
    }

    pub fn load_activity(&mut self, act: Activity) {
        self.repos = act.repos();
        self.checked = vec![true; self.repos.len()];
        self.repo_state = ListState::default().with_selected((!self.repos.is_empty()).then_some(0));
        self.activity = Some(act);
        self.error = None;
        self.screen = Screen::Repos;
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        match self.screen {
            Screen::Setup => self.setup_key(key),
            Screen::Loading => {
                if key.code == KeyCode::Esc {
                    // The worker keeps running but its result will be ignored.
                    self.screen = Screen::Setup;
                    let (tx, rx) = channel();
                    self.tx = tx;
                    self.rx = rx;
                }
            }
            Screen::Repos => self.repos_key(key),
            Screen::Activity => self.activity_key(key),
        }
    }

    // ---------- Setup ----------

    fn setup_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.quit = true,
            KeyCode::Tab | KeyCode::Down => self.focus = self.focus.step(1),
            KeyCode::BackTab | KeyCode::Up => self.focus = self.focus.step(-1),
            KeyCode::Enter => self.start_fetch(),
            KeyCode::Left if self.focus == Field::Month => self.shift_month(-1),
            KeyCode::Right if self.focus == Field::Month => self.shift_month(1),
            KeyCode::Char('h') if self.focus == Field::Month => self.shift_month(-1),
            KeyCode::Char('l') if self.focus == Field::Month => self.shift_month(1),
            KeyCode::Char('q') if self.focus == Field::Month => self.quit = true,
            KeyCode::Backspace => {
                if let Some(s) = self.input_mut() {
                    s.pop();
                }
            }
            KeyCode::Char(c) => {
                let focus = self.focus;
                if let Some(s) = self.input_mut() {
                    let ok = match focus {
                        Field::User => c.is_ascii_alphanumeric() || c == '-',
                        _ => c.is_ascii_digit() || c == '-',
                    };
                    if ok && s.len() < 39 {
                        s.push(c);
                    }
                }
            }
            _ => {}
        }
    }

    fn input_mut(&mut self) -> Option<&mut String> {
        match self.focus {
            Field::User => Some(&mut self.user),
            Field::From => Some(&mut self.from_input),
            Field::To => Some(&mut self.to_input),
            Field::Month => None,
        }
    }

    fn shift_month(&mut self, d: i32) {
        let (y, m) = self.month;
        let idx = y * 12 + (m as i32 - 1) + d;
        self.month = (idx.div_euclid(12), (idx.rem_euclid(12) + 1) as u32);
        let p = Period::month(self.month.0, self.month.1);
        self.from_input = p.from.to_string();
        self.to_input = p.to.to_string();
    }

    /// Label for the period field: the month, or "Custom range" when dates were edited.
    pub fn period_hint(&self) -> String {
        match self.parse_period() {
            Ok(p) => p.label(),
            Err(e) => e,
        }
    }

    fn parse_period(&self) -> std::result::Result<Period, String> {
        let from = NaiveDate::parse_from_str(self.from_input.trim(), "%Y-%m-%d")
            .map_err(|_| "Invalid 'From' date (use YYYY-MM-DD)".to_string())?;
        let to = NaiveDate::parse_from_str(self.to_input.trim(), "%Y-%m-%d")
            .map_err(|_| "Invalid 'To' date (use YYYY-MM-DD)".to_string())?;
        if to < from {
            return Err("'To' is before 'From'".into());
        }
        Ok(Period { from, to })
    }

    fn start_fetch(&mut self) {
        let user = self.user.trim().to_string();
        if user.is_empty() {
            self.error = Some("Enter a GitHub username.".into());
            self.focus = Field::User;
            return;
        }
        let period = match self.parse_period() {
            Ok(p) => p,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        self.period = period;
        self.error = None;

        if self.demo {
            self.load_activity(crate::demo::activity(&user, &period));
            return;
        }
        let Some(token) = self.token.clone() else {
            self.error =
                Some("No GitHub token found. Set GITHUB_TOKEN or run `gh auth login`.".into());
            return;
        };

        self.screen = Screen::Loading;
        self.status = "Connecting to GitHub…".into();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let progress_tx = std::sync::Mutex::new(tx.clone());
            let progress = move |s: String| {
                let _ = progress_tx.lock().unwrap().send(Msg::Progress(s));
            };
            let result = GitHub::new(token).and_then(|gh| gh.fetch(&user, period, &progress));
            let _ = tx.send(Msg::Done(result));
        });
    }

    // ---------- Repo selection ----------

    fn repos_key(&mut self, key: KeyEvent) {
        let n = self.repos.len();
        let i = self.repo_state.selected().unwrap_or(0);
        match key.code {
            KeyCode::Esc => self.screen = Screen::Setup,
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Down | KeyCode::Char('j') if n > 0 => {
                self.repo_state.select(Some((i + 1) % n))
            }
            KeyCode::Up | KeyCode::Char('k') if n > 0 => {
                self.repo_state.select(Some((i + n - 1) % n))
            }
            KeyCode::Char(' ') if n > 0 => self.checked[i] = !self.checked[i],
            KeyCode::Char('a') => self.checked.iter_mut().for_each(|c| *c = true),
            KeyCode::Char('n') => self.checked.iter_mut().for_each(|c| *c = false),
            KeyCode::Char('i') => self.checked.iter_mut().for_each(|c| *c = !*c),
            KeyCode::Enter => {
                let chosen: Vec<String> = self
                    .repos
                    .iter()
                    .zip(&self.checked)
                    .filter(|(_, c)| **c)
                    .map(|(r, _)| r.name.clone())
                    .collect();
                if chosen.is_empty() {
                    self.flash = Some("Select at least one repository.".into());
                    return;
                }
                if let Some(act) = &self.activity {
                    self.view = act.filtered(&chosen);
                    self.tab = Tab::Summary;
                    self.table_state = TableState::default().with_selected(Some(0));
                    self.flash = None;
                    self.screen = Screen::Activity;
                }
            }
            _ => {}
        }
    }

    pub fn selected_count(&self) -> usize {
        self.checked.iter().filter(|c| **c).count()
    }

    // ---------- Activity ----------

    pub fn tab_len(&self, tab: Tab) -> usize {
        match tab {
            Tab::Summary => self.view.repos().len(),
            Tab::PullRequests => self.view.pull_requests.len(),
            Tab::Reviews => self.view.reviews.len(),
            Tab::Issues => self.view.issues.len(),
            Tab::Discussions => self.view.discussions.len(),
        }
    }

    fn selected_url(&self) -> Option<String> {
        let i = self.table_state.selected()?;
        match self.tab {
            Tab::Summary => self
                .view
                .repos()
                .get(i)
                .map(|r| format!("https://github.com/{}", r.name)),
            Tab::PullRequests => self.view.pull_requests.get(i).map(|x| x.url.clone()),
            Tab::Reviews => self.view.reviews.get(i).map(|x| x.url.clone()),
            Tab::Issues => self.view.issues.get(i).map(|x| x.url.clone()),
            Tab::Discussions => self.view.discussions.get(i).map(|x| x.url.clone()),
        }
    }

    fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
        let sel = (self.tab_len(tab) > 0).then_some(0);
        self.table_state = TableState::default().with_selected(sel);
    }

    fn activity_key(&mut self, key: KeyEvent) {
        let n = self.tab_len(self.tab);
        let i = self.table_state.selected().unwrap_or(0);
        self.flash = None;
        match key.code {
            KeyCode::Esc => self.screen = Screen::Repos,
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => self.set_tab(self.tab.step(1)),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                self.set_tab(self.tab.step(-1))
            }
            KeyCode::Char(c @ '1'..='5') => self.set_tab(Tab::ALL[c as usize - '1' as usize]),
            KeyCode::Down | KeyCode::Char('j') if n > 0 => {
                self.table_state.select(Some((i + 1).min(n - 1)))
            }
            KeyCode::Up | KeyCode::Char('k') if n > 0 => {
                self.table_state.select(Some(i.saturating_sub(1)))
            }
            KeyCode::PageDown if n > 0 => self.table_state.select(Some((i + 10).min(n - 1))),
            KeyCode::PageUp if n > 0 => self.table_state.select(Some(i.saturating_sub(10))),
            KeyCode::Home if n > 0 => self.table_state.select(Some(0)),
            KeyCode::End if n > 0 => self.table_state.select(Some(n - 1)),
            KeyCode::Enter | KeyCode::Char('o') => {
                if let Some(url) = self.selected_url() {
                    self.flash = Some(match open::that_detached(&url) {
                        Ok(()) => format!("Opened {url}"),
                        Err(e) => format!("Could not open browser: {e}"),
                    });
                }
            }
            KeyCode::Char('e') => self.export(),
            _ => {}
        }
    }

    fn export(&mut self) {
        let md = report::markdown(&self.view, &self.period);
        let path = format!("boss-report-{}-{}.md", self.view.user, self.period.slug());
        self.flash = Some(match std::fs::write(&path, md) {
            Ok(()) => format!("Saved {path}"),
            Err(e) => format!("Could not save {path}: {e}"),
        });
    }
}
