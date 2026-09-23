//! Terminal menu: the same actions as the Quickshell one, for when there is no shell running.

use crate::hypr::{self, Monitor};
use crate::plan::{Mode, Setup, Side};
use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};
use ratatui::{DefaultTerminal, Frame};
use std::time::Duration;

const SIDES: [Option<Side>; 5] = [None, Some(Side::Right), Some(Side::Left), Some(Side::Up), Some(Side::Down)];

struct App {
    monitors: Vec<Monitor>,
    /// Physical screens other than the primary one.
    externals: Vec<String>,
    primary: Option<String>,
    /// 0 = all external screens, n = externals[n - 1].
    target: usize,
    mode: ListState,
    side: usize,
    message: Option<(String, bool)>,
}

pub fn run() -> Result<()> {
    let mut app = App {
        monitors: Vec::new(),
        externals: Vec::new(),
        primary: None,
        target: 0,
        mode: ListState::default().with_selected(Some(0)),
        side: 0,
        message: None,
    };
    app.refresh()?;
    if let Some(i) = app.setup().ok().and_then(|s| s.current_mode()).and_then(|m| Mode::ALL.iter().position(|&x| x == m)) {
        app.mode.select(Some(i));
    }
    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}

impl App {
    fn refresh(&mut self) -> Result<()> {
        self.monitors = hypr::monitors()?;
        match Setup::resolve(&self.monitors, None, &[]) {
            Ok(s) => {
                self.primary = Some(s.primary.name);
                self.externals = s.targets.into_iter().map(|m| m.name).collect();
            }
            Err(_) => {
                self.primary = self.monitors.iter().find(|m| m.is_internal()).map(|m| m.name.clone());
                self.externals.clear();
            }
        }
        self.target = self.target.min(self.externals.len());
        Ok(())
    }

    fn setup(&self) -> Result<Setup> {
        let outputs = match self.target {
            0 => Vec::new(),
            n => vec![self.externals[n - 1].clone()],
        };
        Setup::resolve(&self.monitors, self.primary.as_deref(), &outputs)
    }

    fn selected_mode(&self) -> Mode {
        Mode::ALL[self.mode.selected().unwrap_or(0)]
    }

    fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|f| self.draw(f))?;
            // Poll so hot-plugged screens show up without a key press.
            if !event::poll(Duration::from_secs(1))? {
                let _ = self.refresh();
                continue;
            }
            let Event::Key(key) = event::read()? else { continue };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Up | KeyCode::Char('k') => self.mode.select_previous(),
                KeyCode::Down | KeyCode::Char('j') => {
                    self.mode.select(Some((self.mode.selected().unwrap_or(0) + 1).min(Mode::ALL.len() - 1)))
                }
                KeyCode::Tab | KeyCode::Char('t') => self.target = (self.target + 1) % (self.externals.len() + 1),
                KeyCode::Right | KeyCode::Char('l') => self.side = (self.side + 1) % SIDES.len(),
                KeyCode::Left | KeyCode::Char('h') => self.side = (self.side + SIDES.len() - 1) % SIDES.len(),
                KeyCode::Enter => self.apply(self.selected_mode()),
                KeyCode::Char(c @ '1'..='4') => {
                    let i = c as usize - '1' as usize;
                    self.mode.select(Some(i));
                    self.apply(Mode::ALL[i]);
                }
                KeyCode::Char('r') => {
                    self.message = Some(match hypr::reload() {
                        Ok(()) => ("Back to the Hyprland config".into(), false),
                        Err(e) => (format!("{e:#}"), true),
                    });
                    std::thread::sleep(Duration::from_millis(300));
                    let _ = self.refresh();
                }
                _ => {}
            }
        }
    }

    fn apply(&mut self, mode: Mode) {
        let side = if mode == Mode::Mirror || mode == Mode::InternalOnly { None } else { SIDES[self.side] };
        let result = self.setup().and_then(|s| crate::apply(&self.monitors, &s, mode, side));
        self.message = Some(match result {
            Ok(w) if w.is_empty() => (format!("{} applied", mode.label()), false),
            Ok(w) => (w.join(" · "), true),
            Err(e) => (format!("{e:#}"), true),
        });
        if let Err(e) = self.refresh() {
            self.message = Some((format!("{e:#}"), true));
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let screens = self.monitors.iter().filter(|m| !m.is_virtual()).count() as u16;
        let [screens_area, target_area, modes_area, help_area] = Layout::vertical([
            Constraint::Length(screens + 2),
            Constraint::Length(3),
            Constraint::Length(Mode::ALL.len() as u16 + 2),
            Constraint::Min(3),
        ])
        .areas(f.area());

        let lines: Vec<Line> = self
            .monitors
            .iter()
            .filter(|m| !m.is_virtual())
            .map(|m| {
                let (dot, state) = if m.disabled {
                    (Span::from("○ ").dark_gray(), "off".to_string())
                } else if m.is_mirroring() {
                    let src = self.monitors.iter().find(|s| s.id.to_string() == m.mirror_of);
                    (Span::from("◐ ").yellow(), format!("mirrors {}", src.map_or("?", |s| &s.name)))
                } else {
                    (Span::from("● ").green(), "on".to_string())
                };
                let primary = if Some(&m.name) == self.primary.as_ref() { "  primary" } else { "" };
                Line::from(vec![
                    dot,
                    Span::from(format!("{:<10}", m.name)).bold(),
                    Span::from(format!("{:<18}{:<14}", m.mode_string(), state)),
                    Span::from(m.description.clone()).dark_gray(),
                    Span::from(primary).cyan(),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(lines).block(Block::bordered().title(" switchout · screens ")), screens_area);

        let target = match self.target {
            0 if self.externals.is_empty() => "no external screen".to_string(),
            0 => "all external screens".to_string(),
            n => self.externals[n - 1].clone(),
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::from("‹ "), Span::from(target).bold(), Span::from(" ›")]))
                .block(Block::bordered().title(" target (Tab) ")),
            target_area,
        );

        let current = self.setup().ok().and_then(|s| s.current_mode());
        let side = match SIDES[self.side] {
            None => "keep position",
            Some(Side::Right) => "right",
            Some(Side::Left) => "left",
            Some(Side::Up) => "above",
            Some(Side::Down) => "below",
        };
        let items: Vec<ListItem> = Mode::ALL
            .iter()
            .enumerate()
            .map(|(i, &m)| {
                let mut spans = vec![Span::from(format!("{}  {:<15}", i + 1, m.label()))];
                if matches!(m, Mode::Extend | Mode::ExternalOnly) {
                    spans.push(Span::from(format!("‹ {side} ›  ")).dark_gray());
                }
                if Some(m) == current {
                    spans.push(Span::from("current").green());
                }
                ListItem::new(Line::from(spans))
            })
            .collect();
        let list = List::new(items)
            .block(Block::bordered().title(" mode "))
            .highlight_style(Style::new().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("▶ ");
        f.render_stateful_widget(list, modes_area, &mut self.mode);

        let mut help = vec![Line::from(
            "↑↓ mode · Tab target · ←→ side · Enter/1-4 apply · r reset to config · q quit".dark_gray(),
        )];
        if let Some((msg, error)) = &self.message {
            help.push(if *error { Line::from(msg.clone()).red() } else { Line::from(msg.clone()).green() });
        }
        f.render_widget(Paragraph::new(help).block(Block::bordered()), help_area);
    }
}
