use crate::tui::state::{
    App, ContainerWindowState, Dialog, ExecutionPhase, Focus, LastContainerSummary,
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// Top-level render function: draws the full TUI for one frame.
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Determine if we need a minimized container bar or a summary bar.
    let show_minimized_bar = app.container_window == ContainerWindowState::Minimized;
    let show_summary = !show_minimized_bar
        && app.container_window == ContainerWindowState::Hidden
        && app.last_container_summary.is_some();
    let extra_bar_height = if show_minimized_bar || show_summary { 3 } else { 0 };

    // Outer layout: execution section (top) + optional bar + status bar + command box + suggestions.
    let constraints = if extra_bar_height > 0 {
        vec![
            Constraint::Min(5),                       // execution window (grows)
            Constraint::Length(extra_bar_height),      // minimized container bar or summary
            Constraint::Length(1),                     // status / hint bar
            Constraint::Length(3),                     // command input box
            Constraint::Length(1),                     // autocomplete suggestions
        ]
    } else {
        vec![
            Constraint::Min(5),    // execution window (grows)
            Constraint::Length(1), // status / hint bar
            Constraint::Length(3), // command input box
            Constraint::Length(1), // autocomplete suggestions
        ]
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let (exec_area, status_idx, cmd_idx, suggest_idx) = if extra_bar_height > 0 {
        (chunks[0], 2, 3, 4)
    } else {
        (chunks[0], 1, 2, 3)
    };

    draw_exec_window(frame, app, exec_area);

    // Draw optional minimized container bar or summary.
    if show_minimized_bar {
        draw_minimized_container_bar(frame, app, chunks[1]);
    } else if show_summary {
        draw_container_summary(frame, app.last_container_summary.as_ref().unwrap(), chunks[1]);
    }

    draw_status_bar(frame, app, chunks[status_idx]);
    draw_command_box(frame, app, chunks[cmd_idx]);
    draw_suggestions(frame, app, chunks[suggest_idx]);

    // Container window overlays on top of the execution window when maximized.
    if app.container_window == ContainerWindowState::Maximized {
        draw_container_window(frame, app, exec_area);
    }

    // Dialogs are drawn on top (centered, floating).
    if app.dialog != Dialog::None {
        draw_dialog(frame, app, area);
    }
}

/// Calculate the inner dimensions of the container window for a given terminal size.
///
/// This mirrors the layout used in `draw_container_window` so the vt100 parser
/// and PTY are sized to match the actual rendered area.
pub fn calculate_container_inner_size(term_cols: u16, term_rows: u16) -> (u16, u16) {
    // Match the outer layout: exec window takes all vertical space minus fixed rows.
    // Fixed rows: status bar (1) + command box (3) + suggestions (1) = 5
    let exec_height = term_rows.saturating_sub(5);
    // Container window: 90% of exec area height, at least 5 rows.
    let container_height = (exec_height * 90 / 100).max(5);
    // Container area has 1-column margin on each side of the exec area.
    let container_width = term_cols.saturating_sub(2);
    // Inner area excludes borders (1 row/col on each side).
    let inner_rows = container_height.saturating_sub(2);
    let inner_cols = container_width.saturating_sub(2);
    (inner_cols, inner_rows)
}

// --- Execution window (outer window) ---

fn draw_exec_window(frame: &mut Frame, app: &App, area: Rect) {
    let border_color = app.window_border_color();
    let border_style = Style::default().fg(border_color);

    // Calculate how many visual rows fit in the window (subtract borders).
    let inner_height = area.height.saturating_sub(2) as usize;

    let phase_label = match &app.phase {
        ExecutionPhase::Idle => " aspec ".to_string(),
        ExecutionPhase::Running { command } => format!(" ● running: {} ", command),
        ExecutionPhase::Done { command } => format!(" ✓ done: {} ", command),
        ExecutionPhase::Error { command, exit_code } => {
            format!(" ✗ error: {} (exit {}) ", command, exit_code)
        }
    };

    let block = Block::default()
        .title(phase_label)
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    let inner_width = area.width.saturating_sub(2) as usize; // exclude borders

    let lines: Vec<Line> = if app.output_lines.is_empty() {
        if matches!(app.phase, ExecutionPhase::Idle) {
            vec![
                Line::from(""),
                Line::from(vec![Span::styled(
                    "  Welcome to aspec.",
                    Style::default().fg(Color::DarkGray),
                )]),
                Line::from(vec![Span::styled(
                    "  Running `aspec ready` to check your environment...",
                    Style::default().fg(Color::DarkGray),
                )]),
            ]
        } else {
            vec![]
        }
    } else {
        app.output_lines
            .iter()
            .map(|l| Line::from(l.as_str()))
            .collect()
    };

    // Calculate how many visual rows the content takes, using display width
    // (via Line::width()) instead of byte length.
    let total_visual: usize = if inner_width == 0 {
        lines.len()
    } else {
        lines
            .iter()
            .map(|l| {
                let w = l.width();
                if w == 0 { 1 } else { (w + inner_width - 1) / inner_width }
            })
            .sum()
    };
    let max_scroll = total_visual.saturating_sub(inner_height);
    let effective_offset = app.scroll_offset.min(max_scroll);
    let scroll_y = max_scroll.saturating_sub(effective_offset);

    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll_y as u16, 0));
    frame.render_widget(para, area);
}

// --- Container window (overlay on top of outer window) ---

fn draw_container_window(frame: &mut Frame, app: &App, outer_area: Rect) {
    // Container window takes 90% of the outer window, anchored to bottom.
    let container_height = (outer_area.height * 90 / 100).max(5);
    let buffer_top = outer_area.height.saturating_sub(container_height);
    let container_area = Rect {
        x: outer_area.x + 1,
        y: outer_area.y + buffer_top,
        width: outer_area.width.saturating_sub(2),
        height: container_height,
    };

    // Clear the area under the container window.
    frame.render_widget(Clear, container_area);

    // Build title strings.
    let agent_name = app
        .container_info
        .as_ref()
        .map(|i| i.agent_display_name.as_str())
        .unwrap_or("Agent");
    let left_title = format!(" \u{1F512} {} (containerized) ", agent_name);

    let right_title = build_stats_title(app);

    let block = Block::default()
        .title(Line::from(left_title).alignment(Alignment::Left))
        .title(Line::from(right_title).alignment(Alignment::Right))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Green));

    let inner = block.inner(container_area);
    frame.render_widget(block, container_area);

    // Render the vt100 terminal emulator screen into the inner area.
    if let Some(ref parser) = app.vt100_parser {
        render_vt100_screen(frame, parser.screen(), inner);
    }
}

/// Render a vt100 screen into a ratatui buffer area, preserving colors,
/// bold/italic/underline, and cursor position.
fn render_vt100_screen(frame: &mut Frame, screen: &vt100::Screen, area: Rect) {
    let buf = frame.buffer_mut();
    let rows = area.height as usize;
    let cols = area.width as usize;
    let screen_rows = screen.size().0 as usize;
    let screen_cols = screen.size().1 as usize;

    for row in 0..rows.min(screen_rows) {
        let mut col = 0;
        while col < cols.min(screen_cols) {
            let cell = screen.cell(row as u16, col as u16);
            let x = area.x + col as u16;
            let y = area.y + row as u16;

            if let Some(cell) = cell {
                let contents = cell.contents();
                let mut style = Style::default();
                style = style.fg(convert_vt100_color(cell.fgcolor()));
                style = style.bg(convert_vt100_color(cell.bgcolor()));
                if cell.bold() {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if cell.italic() {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                if cell.underline() {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
                if cell.inverse() {
                    style = style.add_modifier(Modifier::REVERSED);
                }

                if contents.is_empty() {
                    buf[(x, y)].set_symbol(" ").set_style(style);
                } else {
                    buf[(x, y)].set_symbol(&contents).set_style(style);
                }
            }

            col += 1;
        }
    }

    // Render cursor position.
    if !screen.hide_cursor() {
        let (cursor_row, cursor_col) = screen.cursor_position();
        let cx = area.x + cursor_col;
        let cy = area.y + cursor_row;
        if cx < area.x + area.width && cy < area.y + area.height {
            frame.set_cursor_position((cx, cy));
        }
    }
}

/// Convert a vt100 color to a ratatui color.
fn convert_vt100_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

// --- Minimized container bar ---

fn draw_minimized_container_bar(frame: &mut Frame, app: &App, area: Rect) {
    let agent_name = app
        .container_info
        .as_ref()
        .map(|i| i.agent_display_name.as_str())
        .unwrap_or("Agent");
    let stats_title = build_stats_title(app);

    let content = format!(
        "\u{1F512} {} | {}",
        agent_name,
        stats_title.trim()
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Green));

    let para = Paragraph::new(Line::from(vec![Span::styled(
        format!(" {}", content),
        Style::default().fg(Color::Green),
    )]))
    .block(block);

    frame.render_widget(para, area);
}

// --- Container summary bar (after container exits) ---

fn draw_container_summary(frame: &mut Frame, summary: &LastContainerSummary, area: Rect) {
    let exit_text = if summary.exit_code == 0 {
        "exit 0".to_string()
    } else {
        format!("exit {}", summary.exit_code)
    };

    let content = format!(
        " {} | {} | avg {} | avg {} | {} | {}",
        summary.agent_display_name,
        summary.container_name,
        summary.avg_cpu,
        summary.avg_memory,
        summary.total_time,
        exit_text,
    );

    // Use a custom border set with dashed lines for the summary.
    let border_set = ratatui::symbols::border::Set {
        top_left: "╭",
        top_right: "╮",
        bottom_left: "╰",
        bottom_right: "╯",
        horizontal_top: "╌",
        horizontal_bottom: "╌",
        vertical_left: "┆",
        vertical_right: "┆",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(border_set)
        .border_style(Style::default().fg(Color::DarkGray));

    let color = if summary.exit_code == 0 {
        Color::DarkGray
    } else {
        Color::Red
    };

    let para = Paragraph::new(Line::from(vec![Span::styled(
        content,
        Style::default().fg(color),
    )]))
    .block(block);

    frame.render_widget(para, area);
}

/// Build the right-side title for the container window: "name | cpu | mem | time"
fn build_stats_title(app: &App) -> String {
    let info = match &app.container_info {
        Some(i) => i,
        None => return String::new(),
    };

    let elapsed = info.start_time.elapsed().as_secs();
    let time_str = crate::tui::state::format_duration(elapsed);

    if let Some(ref stats) = info.latest_stats {
        format!(
            " {} | {} | {} | {} ",
            stats.name, stats.cpu_percent, stats.memory, time_str
        )
    } else {
        format!(" {} | ... | ... | {} ", info.container_name, time_str)
    }
}

// --- Status / hint bar ---

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let spans: Vec<Span> = match (&app.phase, &app.focus, &app.container_window) {
        // Container maximized + window focused: Esc to minimize.
        (ExecutionPhase::Running { .. }, Focus::ExecutionWindow, ContainerWindowState::Maximized) => {
            vec![Span::styled(
                " Press Esc to minimize the container window ",
                Style::default().fg(Color::Yellow),
            )]
        }

        // Container minimized + window focused: hints for scrolling + c to restore.
        (ExecutionPhase::Running { .. }, Focus::ExecutionWindow, ContainerWindowState::Minimized) => {
            vec![Span::styled(
                " ↑/↓ scroll  ·  b/e jump  ·  c restore container  ·  Esc deselect ",
                Style::default().fg(Color::DarkGray),
            )]
        }

        // Running + window selected (no container): Esc to deselect.
        (ExecutionPhase::Running { .. }, Focus::ExecutionWindow, ContainerWindowState::Hidden) => {
            vec![Span::styled(
                " Press Esc to deselect the window ",
                Style::default().fg(Color::Yellow),
            )]
        }

        // Running + command box: ↑ to focus the window.
        (ExecutionPhase::Running { .. }, Focus::CommandBox, _) => vec![Span::styled(
            " Press ↑ to focus the window ",
            Style::default().fg(Color::DarkGray),
        )],

        // Done + window selected: Esc to deselect; ↑/↓ to scroll; b/e to jump.
        (ExecutionPhase::Done { .. }, Focus::ExecutionWindow, _) => vec![Span::styled(
            " ↑/↓ scroll  ·  b/e jump  ·  Esc deselect ",
            Style::default().fg(Color::DarkGray),
        )],

        // Done + command box: ↑ to focus the window.
        (ExecutionPhase::Done { .. }, Focus::CommandBox, _) => vec![Span::styled(
            " Press ↑ to focus the window ",
            Style::default().fg(Color::DarkGray),
        )],

        // Error + window selected: exit code + Esc + scroll hint.
        (ExecutionPhase::Error { exit_code, .. }, Focus::ExecutionWindow, _) => vec![
            Span::styled(
                format!(" Exit code: {} ", exit_code),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " ·  ↑/↓ scroll  ·  b/e jump  ·  Esc deselect ",
                Style::default().fg(Color::DarkGray),
            ),
        ],

        // Error + command box: exit code always visible + ↑ to focus.
        (ExecutionPhase::Error { exit_code, .. }, Focus::CommandBox, _) => vec![
            Span::styled(
                format!(" Exit code: {} ", exit_code),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " ·  Press ↑ to focus the window ",
                Style::default().fg(Color::DarkGray),
            ),
        ],

        _ => vec![],
    };

    let bar = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Black));
    frame.render_widget(bar, area);
}

// --- Command input box ---

fn draw_command_box(frame: &mut Frame, app: &App, area: Rect) {
    let is_active = app.focus == Focus::CommandBox
        && !matches!(app.phase, ExecutionPhase::Running { .. });

    let border_color = if is_active {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .title(if is_active { " command " } else { " command (inactive) " })
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    // Show error or current input.
    let content = if let Some(ref err) = app.input_error {
        vec![Line::from(vec![Span::styled(
            format!("  {}", err),
            Style::default().fg(Color::Red),
        )])]
    } else {
        let prefix = Span::styled("> ", Style::default().fg(Color::Cyan));
        let text = Span::raw(app.input.replace('\n', "↵"));
        vec![Line::from(vec![prefix, text])]
    };

    let para = Paragraph::new(content).block(block);
    frame.render_widget(para, area);

    // Render cursor when active.
    if is_active && app.input_error.is_none() {
        let cursor_x = area.x + 1 + 2 + app.cursor_col as u16; // border + "> "
        let cursor_y = area.y + 1; // inside border
        if cursor_x < area.x + area.width - 1 {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

// --- Autocomplete suggestions ---

fn draw_suggestions(frame: &mut Frame, app: &App, area: Rect) {
    if app.suggestions.is_empty() || app.focus != Focus::CommandBox {
        return;
    }

    let spans: Vec<Span> = app
        .suggestions
        .iter()
        .enumerate()
        .flat_map(|(i, s)| {
            let sep = if i == 0 {
                Span::raw("  ")
            } else {
                Span::styled("  ·  ", Style::default().fg(Color::DarkGray))
            };
            vec![
                sep,
                Span::styled(s.as_str(), Style::default().fg(Color::Cyan)),
            ]
        })
        .collect();

    let para = Paragraph::new(Line::from(spans))
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(para, area);
}

// --- Modal dialogs ---

fn draw_dialog(frame: &mut Frame, app: &App, area: Rect) {
    let (title, body) = match &app.dialog {
        Dialog::QuitConfirm => (
            " Quit aspec? ",
            "  Are you sure you want to quit? [y/n]  ".to_string(),
        ),
        Dialog::MountScope { git_root, cwd } => (
            " Mount Scope ",
            format!(
                "  Git root: {}\n  CWD:      {}\n\n  Mount Git root (r) or CWD only (c)? [r/c]  ",
                git_root.display(),
                cwd.display()
            ),
        ),
        Dialog::AgentAuth { agent, git_root } => (
            " Agent Credentials ",
            format!(
                "  Mount {} credentials into the container?\n  (saved for this repo: {})\n\n  [y/n]  ",
                agent,
                git_root.display()
            ),
        ),
        Dialog::NewKindSelect => (
            " New Work Item — Type ",
            "  Select work item type:\n\n  1) Feature\n  2) Bug\n  3) Task\n\n  [1/2/3 or Esc to cancel]  ".to_string(),
        ),
        Dialog::NewTitleInput { kind, title } => (
            " New Work Item — Title ",
            format!(
                "  Type: {}\n\n  Enter title: {}\n\n  [Enter to confirm, Esc to cancel]  ",
                kind.as_str(),
                title
            ),
        ),
        Dialog::None => return,
    };

    let popup_width = 60u16.min(area.width.saturating_sub(4));
    let popup_height = 7u16.min(area.height.saturating_sub(4));
    let popup = centered_rect(popup_width, popup_height, area);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(title)
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow));

    let para = Paragraph::new(body.as_str())
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(para, popup);
}

/// Return a centered rectangle of the given size within `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect { x, y, width: width.min(area.width), height: height.min(area.height) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::App;
    use ratatui::{backend::TestBackend, Terminal};

    /// Helper: render the app into a TestBackend and return the text content
    /// of the execution window's inner area (excluding borders).
    fn render_exec_window_lines(app: &App, width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw(f, app))
            .unwrap();
        let buf = terminal.backend().buffer();
        // The exec window occupies the top area. Layout: Min(5), Len(1), Len(3), Len(1).
        // So exec window height = total_height - 5 (status bar + cmd box + suggestions).
        let exec_height = height.saturating_sub(5);
        // Inner area excludes borders (1 row top, 1 row bottom, 1 col left, 1 col right).
        let inner_top = 1u16;
        let inner_left = 1u16;
        let inner_width = width.saturating_sub(2);
        let inner_rows = exec_height.saturating_sub(2);

        let mut lines = Vec::new();
        for row in inner_top..(inner_top + inner_rows) {
            let mut line = String::new();
            for col in inner_left..(inner_left + inner_width) {
                let cell = &buf[(col, row)];
                line.push_str(cell.symbol());
            }
            lines.push(line.trim_end().to_string());
        }
        lines
    }

    #[test]
    fn scroll_changes_visible_content_in_done_state() {
        let mut app = App::new();
        // Terminal: 40 wide, 15 tall → exec window = 15-5=10 rows → inner = 8 rows
        // Add 20 lines of output so there's content to scroll through.
        for i in 0..20 {
            app.output_lines.push(format!("line {}", i));
        }
        app.phase = ExecutionPhase::Done {
            command: "ready".into(),
        };
        app.focus = Focus::ExecutionWindow;

        // scroll_offset=0 → should show the LAST 8 lines (lines 12-19).
        app.scroll_offset = 0;
        let view0 = render_exec_window_lines(&app, 40, 15);
        assert!(
            view0.iter().any(|l| l.contains("line 19")),
            "scroll_offset=0 should show line 19 (newest). Got: {:?}",
            view0
        );
        assert!(
            !view0.iter().any(|l| l.contains("line 0")),
            "scroll_offset=0 should NOT show line 0 (oldest). Got: {:?}",
            view0
        );

        // scroll_offset=5 → should show earlier content.
        app.scroll_offset = 5;
        let view5 = render_exec_window_lines(&app, 40, 15);
        assert!(
            view5.iter().any(|l| l.contains("line 7")),
            "scroll_offset=5 should show line 7. Got: {:?}",
            view5
        );

        // The two views must differ.
        assert_ne!(
            view0, view5,
            "Scrolling must change the visible content"
        );

        // scroll_offset=max → should show the FIRST lines.
        app.scroll_offset = 20;
        let view_top = render_exec_window_lines(&app, 40, 15);
        assert!(
            view_top.iter().any(|l| l.contains("line 0")),
            "scroll_offset=max should show line 0 (oldest). Got: {:?}",
            view_top
        );
    }

    #[test]
    fn unicode_lines_do_not_cause_scroll_overshoot() {
        let mut app = App::new();
        // Box-drawing chars: "─" is 3 bytes but 1 display column.
        for i in 0..10 {
            app.output_lines
                .push(format!("──── step {} ────", i));
        }
        app.phase = ExecutionPhase::Done {
            command: "ready".into(),
        };
        app.focus = Focus::ExecutionWindow;
        app.scroll_offset = 0;

        // 40 wide, 15 tall → inner = 8 rows. 10 lines of ~16 display cols each.
        let view = render_exec_window_lines(&app, 40, 15);
        assert!(
            view.iter().any(|l| l.contains("step 9")),
            "Newest line must be visible with Unicode content. Got: {:?}",
            view
        );
    }

    #[test]
    fn container_summary_renders_after_container_exit() {
        let mut app = App::new();
        app.phase = ExecutionPhase::Done { command: "implement 0001".into() };
        app.last_container_summary = Some(LastContainerSummary {
            agent_display_name: "Claude Code".into(),
            container_name: "aspec-test".into(),
            avg_cpu: "5.0%".into(),
            avg_memory: "200MiB".into(),
            total_time: "12m".into(),
            exit_code: 0,
        });

        // Render with enough space to include the summary bar.
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let buf = terminal.backend().buffer();

        // Collect all text from the buffer to verify summary content appears.
        let mut all_text = String::new();
        for row in 0..20 {
            for col in 0..80 {
                let cell = &buf[(col, row)];
                all_text.push_str(cell.symbol());
            }
        }
        assert!(
            all_text.contains("Claude Code"),
            "Summary should contain agent name. Got buffer text."
        );
        assert!(
            all_text.contains("aspec-test"),
            "Summary should contain container name."
        );
    }

    #[test]
    fn container_window_renders_when_maximized() {
        let mut app = App::new();
        app.phase = ExecutionPhase::Running { command: "implement 0001".into() };
        app.focus = Focus::ExecutionWindow;
        // Use size matching what TestBackend(80,25) would produce.
        let (inner_cols, inner_rows) = calculate_container_inner_size(80, 25);
        app.start_container("aspec-test".into(), "Claude Code".into(), inner_cols, inner_rows);

        // Feed data through the vt100 parser.
        if let Some(ref mut parser) = app.vt100_parser {
            parser.process(b"Hello from container\r\n");
        }

        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let buf = terminal.backend().buffer();

        let mut all_text = String::new();
        for row in 0..25 {
            for col in 0..80 {
                let cell = &buf[(col, row)];
                all_text.push_str(cell.symbol());
            }
        }
        // Container window should show agent name and "containerized".
        assert!(
            all_text.contains("containerized"),
            "Container window should show '(containerized)' label"
        );
        // Container output should be visible via vt100 rendering.
        assert!(
            all_text.contains("Hello from container"),
            "Container output should be rendered in the container window"
        );
    }

    #[test]
    fn minimized_container_bar_renders() {
        let mut app = App::new();
        app.phase = ExecutionPhase::Running { command: "implement 0001".into() };
        app.focus = Focus::ExecutionWindow;
        app.start_container("aspec-test".into(), "Claude Code".into(), 78, 18);
        app.container_window = ContainerWindowState::Minimized;

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let buf = terminal.backend().buffer();

        let mut all_text = String::new();
        for row in 0..20 {
            for col in 0..80 {
                let cell = &buf[(col, row)];
                all_text.push_str(cell.symbol());
            }
        }
        assert!(
            all_text.contains("Claude Code"),
            "Minimized bar should contain agent name"
        );
    }

    #[test]
    fn calculate_container_inner_size_reasonable_values() {
        let (cols, rows) = calculate_container_inner_size(80, 25);
        // exec_height = 25 - 5 = 20
        // container_height = 20 * 90 / 100 = 18
        // container_width = 80 - 2 = 78
        // inner_rows = 18 - 2 = 16
        // inner_cols = 78 - 2 = 76
        assert_eq!(cols, 76);
        assert_eq!(rows, 16);
    }
}
