//! Minimal, aesthetically-pleasing TUI for chatting with a [`MicroAgent`].
//!
//! The TUI is intentionally backend-agnostic: it does not own the agent, but
//! accepts a closure that knows how to start a new run for a given prompt and
//! (optional) session id. This makes it easy to plug in any concrete
//! [`MicroAgent<Ctx>`](microagents_core::agent::MicroAgent) since `Agent::run`
//! consumes the agent — the closure can rebuild it on every turn while keeping
//! continuity through the session id.
//!
//! ```ignore
//! use microagents_cli::tui;
//! tui::run(|prompt, session_id| async move {
//!     build_agent().run(prompt, session_id).await
//! }).await?;
//! ```
use std::{
    future::Future,
    io::{self, Stdout},
    time::Duration,
};

use futures_util::StreamExt;
use microagents_core::types::{AgentError, RunStream};
use microagents_events::{AgentEventAny, DeltaType, types::ToolResult};
use ratatui::{
    Frame, Terminal,
    crossterm::{
        event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    },
    layout::{Constraint, Direction, Layout, Rect},
    prelude::CrosstermBackend,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Padding, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
};
use tokio::sync::mpsc;

/// Palette tuned for both light and dark terminals.
mod theme {
    use ratatui::style::Color;

    pub const ACCENT: Color = Color::Rgb(137, 180, 250); // soft blue
    pub const ACCENT_SOFT: Color = Color::Rgb(116, 199, 236);
    pub const USER: Color = Color::Rgb(166, 227, 161); // mint
    pub const ASSISTANT: Color = Color::Rgb(205, 214, 244); // off-white
    pub const THINKING: Color = Color::Rgb(147, 153, 178); // muted
    pub const TOOL: Color = Color::Rgb(249, 226, 175); // soft yellow
    pub const TOOL_OK: Color = Color::Rgb(166, 227, 161);
    pub const TOOL_ERR: Color = Color::Rgb(243, 139, 168);
    pub const SKILL: Color = Color::Rgb(203, 166, 247); // lavender
    pub const DIM: Color = Color::Rgb(108, 112, 134);
    pub const ERROR: Color = Color::Rgb(243, 139, 168);
}

/// A single rendered line in the chat transcript.
#[derive(Debug, Clone)]
enum Msg {
    User(String),
    Assistant(String),
    Thinking(String),
    ToolCall { name: String, input: String },
    ToolResult(ToolResult),
    Skill(String),
    Session(String),
    Error(String),
}

#[derive(Debug)]
enum UiEvent {
    Agent(AgentEventAny),
    AgentError(AgentError),
    /// The current run has finished — the input is unlocked.
    RunFinished,
}

struct App {
    input: String,
    cursor: usize,
    transcript: Vec<Msg>,
    session_id: Option<String>,
    busy: bool,
    scroll: u16,
    auto_scroll: bool,
    quit: bool,
    /// Number of lines the last-rendered transcript actually used. Used to clamp scrolling.
    last_content_height: u16,
    last_viewport_height: u16,
}

impl App {
    fn new() -> Self {
        Self {
            input: String::new(),
            cursor: 0,
            transcript: Vec::new(),
            session_id: None,
            busy: false,
            scroll: 0,
            auto_scroll: true,
            quit: false,
            last_content_height: 0,
            last_viewport_height: 0,
        }
    }

    fn push(&mut self, block: Msg) {
        self.transcript.push(block);
        if self.auto_scroll {
            self.scroll = u16::MAX; // clamp in draw
        }
    }

    /// Append streaming text to the current assistant block, creating one if needed.
    fn append_assistant_delta(&mut self, delta: &str, thinking: bool) {
        if delta.is_empty() {
            return;
        }
        let target_is_match = match self.transcript.last() {
            Some(Msg::Assistant(_)) if !thinking => true,
            Some(Msg::Thinking(_)) if thinking => true,
            _ => false,
        };
        if !target_is_match {
            self.transcript.push(if thinking {
                Msg::Thinking(String::new())
            } else {
                Msg::Assistant(String::new())
            });
        }
        match self.transcript.last_mut().unwrap() {
            Msg::Assistant(s) | Msg::Thinking(s) => s.push_str(delta),
            _ => unreachable!(),
        }
        if self.auto_scroll {
            self.scroll = u16::MAX;
        }
    }

    fn apply(&mut self, ev: UiEvent) {
        match ev {
            UiEvent::Agent(e) => self.apply_agent_event(e),
            UiEvent::AgentError(e) => self.push(Msg::Error(e.to_string())),
            UiEvent::RunFinished => self.busy = false,
        }
    }

    fn apply_agent_event(&mut self, ev: AgentEventAny) {
        match ev {
            AgentEventAny::SessionInit(s) => {
                self.session_id = Some(s.session_id.clone());
                let kind = match s.init_type {
                    microagents_events::SessionInitType::Start => "started",
                    microagents_events::SessionInitType::Resume => "resumed",
                };
                self.push(Msg::Session(format!(
                    "session {} • {} • {}/{}",
                    kind,
                    short_id(&s.session_id),
                    s.provider,
                    s.model
                )));
            }
            AgentEventAny::SessionStop(s) => {
                if let Some(err) = s.error {
                    self.push(Msg::Error(err));
                }
                self.push(Msg::Session(format!(
                    "session stopped • {}",
                    if s.success { "ok" } else { "failed" }
                )));
            }
            AgentEventAny::UserPromptSubmit(_) => {
                // The prompt is already echoed locally on send.
            }
            AgentEventAny::StreamDelta(d) => {
                let thinking = matches!(d.delta_type, DeltaType::Thinking);
                self.append_assistant_delta(&d.delta, thinking);
            }
            AgentEventAny::ToolCall(t) => {
                let input = serde_json::to_string(&t.input).unwrap_or_else(|_| "{}".into());
                self.push(Msg::ToolCall {
                    name: t.name,
                    input,
                });
            }
            AgentEventAny::ToolResult(r) => self.push(Msg::ToolResult(r.result)),
            AgentEventAny::SkillLoad(s) => self.push(Msg::Skill(s.skill_name)),
            AgentEventAny::AssistantResponse(r) => {
                // Streamed text is already rendered via deltas. The core does
                // not currently emit `tool.call` events, so derive them from
                // the final assistant response's `tool_calls` field.
                if let Some(calls) = r.tool_calls {
                    for c in calls {
                        let input =
                            serde_json::from_str::<serde_json::Value>(&c.function.arguments)
                                .ok()
                                .and_then(|v| serde_json::to_string(&v).ok())
                                .unwrap_or(c.function.arguments);
                        self.push(Msg::ToolCall {
                            name: c.function.name,
                            input,
                        });
                    }
                }
            }
        }
    }
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

/// Start an interactive TUI session driven by `start_run`.
///
/// `start_run` is called for each user message; it receives the prompt and the
/// current session id (if any) and must return a [`RunStream`] of agent events.
pub async fn run<F, Fut>(mut start_run: F) -> io::Result<()>
where
    F: FnMut(String, Option<String>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<RunStream, AgentError>> + Send + 'static,
{
    let mut terminal = setup_terminal()?;
    let res = event_loop(&mut terminal, &mut start_run).await;
    restore_terminal(&mut terminal)?;
    res
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

async fn event_loop<F, Fut>(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    start_run: &mut F,
) -> io::Result<()>
where
    F: FnMut(String, Option<String>) -> Fut,
    Fut: Future<Output = Result<RunStream, AgentError>> + Send + 'static,
{
    let mut app = App::new();
    let (ui_tx, mut ui_rx) = mpsc::unbounded_channel::<UiEvent>();
    let mut tick = tokio::time::interval(Duration::from_millis(80));

    while !app.quit {
        terminal.draw(|f| draw(f, &mut app))?;

        tokio::select! {
            biased;
            Some(ev) = ui_rx.recv() => {
                app.apply(ev);
                // Drain anything else that's immediately available to keep streams smooth.
                while let Ok(more) = ui_rx.try_recv() {
                    app.apply(more);
                }
            }
            _ = tick.tick() => {
                while event::poll(Duration::from_millis(0))? {
                    if let Event::Key(key) = event::read()? {
                        if key.kind != KeyEventKind::Press { continue; }
                        handle_key(key, &mut app, start_run, &ui_tx).await;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_key<F, Fut>(
    key: event::KeyEvent,
    app: &mut App,
    start_run: &mut F,
    ui_tx: &mpsc::UnboundedSender<UiEvent>,
) where
    F: FnMut(String, Option<String>) -> Fut,
    Fut: Future<Output = Result<RunStream, AgentError>> + Send + 'static,
{
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('c') if ctrl => app.quit = true,
        KeyCode::Char('d') if ctrl && app.input.is_empty() => app.quit = true,
        KeyCode::Esc => app.quit = true,
        KeyCode::Enter if !app.busy => {
            let prompt = app.input.trim().to_string();
            if prompt.is_empty() {
                return;
            }
            if matches!(prompt.as_str(), "/exit" | "/quit") {
                app.quit = true;
                return;
            }
            app.input.clear();
            app.cursor = 0;
            app.push(Msg::User(prompt.clone()));
            app.busy = true;
            app.auto_scroll = true;

            let fut = start_run(prompt, app.session_id.clone());
            let tx = ui_tx.clone();
            tokio::spawn(async move {
                match fut.await {
                    Ok(mut stream) => {
                        while let Some(item) = stream.next().await {
                            match item {
                                Ok(e) => {
                                    if tx.send(UiEvent::Agent(e)).is_err() {
                                        return;
                                    }
                                }
                                Err(e) => {
                                    let _ = tx.send(UiEvent::AgentError(e));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(UiEvent::AgentError(e));
                    }
                }
                let _ = tx.send(UiEvent::RunFinished);
            });
        }
        KeyCode::Char(c) if !ctrl => {
            app.input.insert(app.cursor, c);
            app.cursor += c.len_utf8();
        }
        KeyCode::Backspace => {
            if app.cursor > 0 {
                // remove the previous char (UTF-8 safe via floor_char_boundary equivalent)
                let mut new_cursor = app.cursor - 1;
                while !app.input.is_char_boundary(new_cursor) && new_cursor > 0 {
                    new_cursor -= 1;
                }
                app.input.replace_range(new_cursor..app.cursor, "");
                app.cursor = new_cursor;
            }
        }
        KeyCode::Left => {
            if app.cursor > 0 {
                let mut nc = app.cursor - 1;
                while !app.input.is_char_boundary(nc) && nc > 0 {
                    nc -= 1;
                }
                app.cursor = nc;
            }
        }
        KeyCode::Right => {
            if app.cursor < app.input.len() {
                let mut nc = app.cursor + 1;
                while nc < app.input.len() && !app.input.is_char_boundary(nc) {
                    nc += 1;
                }
                app.cursor = nc;
            }
        }
        KeyCode::Home => app.cursor = 0,
        KeyCode::End => app.cursor = app.input.len(),
        KeyCode::PageUp => {
            app.auto_scroll = false;
            app.scroll = app.scroll.saturating_sub(app.last_viewport_height.max(1));
        }
        KeyCode::PageDown => {
            let max = app
                .last_content_height
                .saturating_sub(app.last_viewport_height);
            app.scroll = app.scroll.saturating_add(app.last_viewport_height).min(max);
            app.auto_scroll = app.scroll >= max;
        }
        _ => {}
    }
}

fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(1),    // chat
            Constraint::Length(3), // input
            Constraint::Length(1), // hint
        ])
        .split(f.area());

    draw_header(f, chunks[0], app);
    draw_transcript(f, chunks[1], app);
    draw_input(f, chunks[2], app);
    draw_hint(f, chunks[3], app);
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let session = app
        .session_id
        .as_deref()
        .map(short_id)
        .unwrap_or_else(|| "—".into());
    let status = if app.busy { "● thinking" } else { "○ idle" };
    let status_color = if app.busy {
        theme::ACCENT_SOFT
    } else {
        theme::DIM
    };

    let title = Line::from(vec![
        Span::styled("✦ ", Style::default().fg(theme::ACCENT)),
        Span::styled(
            "microagents",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("session {}", session),
            Style::default().fg(theme::DIM),
        ),
        Span::raw("  "),
        Span::styled(status, Style::default().fg(status_color)),
    ]);

    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(theme::DIM));
    let p = Paragraph::new(title).block(block).left_aligned();
    f.render_widget(p, area);
}

fn block_lines(block: &Msg) -> Vec<Line<'static>> {
    match block {
        Msg::User(text) => prefixed("  you", theme::USER, text, theme::ASSISTANT),
        Msg::Assistant(text) => prefixed("  agent", theme::ACCENT, text, theme::ASSISTANT),
        Msg::Thinking(text) => {
            let lines = wrap_to_lines(text);
            let mut out = Vec::with_capacity(lines.len() + 1);
            out.push(Line::from(Span::styled(
                "  · thinking",
                Style::default()
                    .fg(theme::THINKING)
                    .add_modifier(Modifier::ITALIC),
            )));
            for l in lines {
                out.push(Line::from(Span::styled(
                    format!("    {}", l),
                    Style::default()
                        .fg(theme::THINKING)
                        .add_modifier(Modifier::ITALIC),
                )));
            }
            out
        }
        Msg::ToolCall { name, input } => {
            let preview = if input.len() > 400 {
                format!("{}…", &input[..400])
            } else {
                input.clone()
            };
            vec![
                Line::from(Span::styled(
                    "  tool",
                    Style::default()
                        .fg(theme::TOOL)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(vec![
                    Span::styled("    ↪ ", Style::default().fg(theme::TOOL)),
                    Span::styled(
                        name.clone(),
                        Style::default()
                            .fg(theme::TOOL)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(preview, Style::default().fg(theme::DIM)),
                ]),
            ]
        }
        Msg::ToolResult(r) => match r {
            ToolResult::Ok(s) => prefixed_inline("  ✓ ", theme::TOOL_OK, s, theme::DIM),
            ToolResult::Err(s) => prefixed_inline("  ✗ ", theme::TOOL_ERR, s, theme::TOOL_ERR),
        },
        Msg::Skill(name) => vec![Line::from(vec![
            Span::styled("  ✧ ", Style::default().fg(theme::SKILL)),
            Span::styled(
                format!("skill loaded: {}", name),
                Style::default().fg(theme::SKILL),
            ),
        ])],
        Msg::Session(msg) => vec![Line::from(Span::styled(
            format!("  ── {}", msg),
            Style::default()
                .fg(theme::DIM)
                .add_modifier(Modifier::ITALIC),
        ))],
        Msg::Error(msg) => vec![Line::from(vec![
            Span::styled("  ! ", Style::default().fg(theme::ERROR)),
            Span::styled(
                msg.clone(),
                Style::default()
                    .fg(theme::ERROR)
                    .add_modifier(Modifier::BOLD),
            ),
        ])],
    }
}

fn prefixed(label: &str, label_color: Color, text: &str, body_color: Color) -> Vec<Line<'static>> {
    let mut out = vec![Line::from(Span::styled(
        label.to_string(),
        Style::default()
            .fg(label_color)
            .add_modifier(Modifier::BOLD),
    ))];
    for l in wrap_to_lines(text) {
        out.push(Line::from(Span::styled(
            format!("    {}", l),
            Style::default().fg(body_color),
        )));
    }
    out
}

fn prefixed_inline(
    prefix: &str,
    prefix_color: Color,
    text: &str,
    body_color: Color,
) -> Vec<Line<'static>> {
    let lines = wrap_to_lines(text);
    if lines.is_empty() {
        return vec![Line::from(Span::styled(
            prefix.to_string(),
            Style::default().fg(prefix_color),
        ))];
    }
    let mut out = Vec::with_capacity(lines.len());
    for (i, l) in lines.into_iter().enumerate() {
        if i == 0 {
            out.push(Line::from(vec![
                Span::styled(
                    prefix.to_string(),
                    Style::default()
                        .fg(prefix_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(l, Style::default().fg(body_color)),
            ]));
        } else {
            out.push(Line::from(Span::styled(
                format!("    {}", l),
                Style::default().fg(body_color),
            )));
        }
    }
    out
}

/// Break text on existing newlines; ratatui's wrap handles line-width wrapping.
fn wrap_to_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split('\n').map(|s| s.to_string()).collect()
}

fn draw_transcript(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::DIM))
        .padding(Padding::new(1, 1, 0, 0));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (i, b) in app.transcript.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        lines.extend(block_lines(b));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Ask anything to get started.",
            Style::default().fg(theme::DIM).add_modifier(Modifier::DIM),
        )));
    }

    // Total height after width-wrapping is hard to compute exactly; approximate
    // by counting logical lines. ratatui's Paragraph::wrap will handle visual
    // wrapping; we clamp scroll using logical line count which is a reasonable
    // proxy for moderately-sized messages.
    let total = lines.len() as u16;
    let viewport = inner.height;
    app.last_content_height = total;
    app.last_viewport_height = viewport;

    let max_scroll = total.saturating_sub(viewport);
    if app.auto_scroll || app.scroll > max_scroll {
        app.scroll = max_scroll;
    }

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((app.scroll, 0));
    f.render_widget(paragraph, inner);

    if total > viewport {
        let mut sb_state = ScrollbarState::new(total as usize).position(app.scroll as usize);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(theme::DIM));
        f.render_stateful_widget(scrollbar, area, &mut sb_state);
    }
}

fn draw_input(f: &mut Frame, area: Rect, app: &App) {
    let border_color = if app.busy { theme::DIM } else { theme::ACCENT };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .padding(Padding::horizontal(1));

    let prompt_marker = Span::styled(
        "▎ ",
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    );

    let body: Span = if app.input.is_empty() && !app.busy {
        Span::styled(
            "Type a message and press Enter…",
            Style::default().fg(theme::DIM).add_modifier(Modifier::DIM),
        )
    } else if app.busy && app.input.is_empty() {
        Span::styled(
            "Waiting for the agent…",
            Style::default()
                .fg(theme::DIM)
                .add_modifier(Modifier::ITALIC),
        )
    } else {
        Span::styled(app.input.clone(), Style::default().fg(theme::ASSISTANT))
    };

    let line = Line::from(vec![prompt_marker, body]);
    let p = Paragraph::new(line).block(block);
    f.render_widget(p, area);

    // Cursor placement (only when accepting input).
    if !app.busy {
        let inner = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        // Visual offset = padding(1) + "▎ "(2) + cursor offset (chars before cursor).
        let prefix_cols = 1 + 2;
        let chars_before = app.input[..app.cursor].chars().count() as u16;
        let cx = inner.x + prefix_cols + chars_before;
        let cy = inner.y;
        if cx < inner.x + inner.width {
            f.set_cursor_position((cx, cy));
        }
    }
}

fn draw_hint(f: &mut Frame, area: Rect, app: &App) {
    let hint = if app.busy {
        "  streaming…  •  Ctrl+C quit"
    } else {
        "  Enter send  •  PgUp/PgDn scroll  •  /exit or Esc to quit"
    };
    let p = Paragraph::new(Line::from(Span::styled(
        hint,
        Style::default().fg(theme::DIM),
    )));
    f.render_widget(p, area);
}
