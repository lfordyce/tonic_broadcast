use std::{cell::RefCell, mem, rc::Rc};
use std::{
    error::Error,
    io::stdout,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use tui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Modifier,
    symbols::{bar, DOT},
    text::Span,
    widgets::{Paragraph, Wrap},
};
use tui::{style::Color, terminal::Frame, Terminal};
use tui::{
    style::Style,
    text::Spans,
    widgets::{Block, Borders, BorderType, Tabs},
};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use tui::backend::CrosstermBackend;
use std::io::Stdout;
use crossterm::event::KeyEvent;
use std::future::Future;
use tokio::sync::mpsc::error::SendError;

enum Event<I> {
    Input(I),
    Tick,
}

#[derive(Copy, Clone, Debug)]
enum MenuItem {
    Home,
    Room,
}

impl From<MenuItem> for usize {
    fn from(input: MenuItem) -> usize {
        match input {
            MenuItem::Home => 0,
            MenuItem::Room => 1,
        }
    }
}

pub struct Ui {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    rx: tokio::sync::mpsc::Receiver<Event<KeyEvent>>,
}

impl Ui {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        enable_raw_mode()?;

        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let tick_rate = Duration::from_millis(200);

        tokio::spawn(async move {
            let mut last_tick = tokio::time::Instant::now();
            loop {
                let timeout = tick_rate
                    .checked_sub(last_tick.elapsed())
                    .unwrap_or_else(|| Duration::from_secs(0));

                if event::poll(timeout).expect("poll works") {
                    if let CEvent::Key(key) = event::read().expect("can read events") {
                        if let Err(_) = tx.send(Event::Input(key)).await {
                            println!("receiver dropped");
                            return;
                        }
                    }
                }

                if last_tick.elapsed() >= tick_rate {
                    if let Ok(_) = tx.send(Event::Tick).await {
                        last_tick = tokio::time::Instant::now();
                    }
                }
            }
        });

        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self { terminal, rx })
    }

    pub async fn render(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let menu_titles = vec!["Home", "Room", "Quit"];
        let mut active_menu_item = MenuItem::Home;
        self.terminal.clear()?;

        loop {
            self.terminal.draw(|rect| {
                let size = rect.size();
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints(
                        [
                            Constraint::Length(3),
                            Constraint::Min(2),
                            Constraint::Length(3),
                        ]
                            .as_ref(),
                    )
                    .split(size);

                let menu = menu_titles
                    .iter()
                    .map(|t| {
                        let (fist, rest) = t.split_at(1);
                        Spans::from(vec![
                            Span::styled(
                                fist,
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::UNDERLINED),
                            ),
                            Span::styled(rest, Style::default().fg(Color::White)),
                        ])
                    })
                    .collect();

                let tabs = Tabs::new(menu)
                    .select(active_menu_item.into())
                    .block(Block::default().title("Menu").borders(Borders::ALL))
                    .style(Style::default().fg(Color::White))
                    .highlight_style(Style::default().fg(Color::Yellow))
                    .divider(Span::raw("|"));

                rect.render_widget(tabs, chunks[0]);
                match active_menu_item {
                    MenuItem::Home => rect.render_widget(render_home(), chunks[1]),
                    MenuItem::Room => {}
                }
            })?;

            match self.rx.recv().await.unwrap() {
                Event::Input(event) => match event.code {
                    KeyCode::Char('q') => {
                        disable_raw_mode()?;
                        self.terminal.show_cursor()?;
                        break;
                    }
                    KeyCode::Char('h') => {}
                    _ => {}
                }
                Event::Tick => {}
            }
        }
        Ok(())
    }
}

fn render_home<'a>() -> Paragraph<'a> {
    let home = Paragraph::new(vec![
        Spans::from(vec![Span::raw("")]),
        Spans::from(vec![Span::raw("Welcome")]),
        Spans::from(vec![Span::raw("")]),
        Spans::from(vec![Span::raw("to")]),
        Spans::from(vec![Span::raw("")]),
        Spans::from(vec![Span::styled(
            "pet-CLI",
            Style::default().fg(Color::LightBlue),
        )]),
        Spans::from(vec![Span::raw("")]),
        Spans::from(vec![Span::raw("Press 'p' to access pets, 'a' to add random new pets and 'd' to delete the currently selected pet.")]),
    ])
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White))
                .title("Home")
                .border_type(BorderType::Plain),
        );
    home
}