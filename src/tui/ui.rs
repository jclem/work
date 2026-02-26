use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Tabs, Wrap};

use super::app::{App, Confirm, DetailView, Tab, TaskViewMode, TreeRow};

const SPINNER_FRAMES: &[&str] = &["◐", "◓", "◑", "◒"];

pub fn draw(frame: &mut Frame, app: &App, tick_count: usize) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);

    draw_tab_bar(frame, app, chunks[0]);

    match app.detail {
        Some(DetailView::TaskLog { .. } | DetailView::EnvironmentLog { .. }) => {
            draw_log_view(frame, app, chunks[1])
        }
        None => match app.tab {
            Tab::Tasks => match app.task_view_mode {
                TaskViewMode::Flat => draw_task_list_flat(frame, app, tick_count, chunks[1]),
                TaskViewMode::Tree => draw_task_list_tree(frame, app, tick_count, chunks[1]),
            },
            Tab::Projects => draw_project_list(frame, app, chunks[1]),
            Tab::Environments => draw_environment_list(frame, app, tick_count, chunks[1]),
            Tab::Daemon => draw_daemon_view(frame, app, tick_count, chunks[1]),
            Tab::Logs => draw_tui_logs_view(frame, app, chunks[1]),
        },
    }

    draw_status_bar(frame, app, chunks[2]);

    if app.create_task_prompt.is_some() {
        draw_create_task_prompt(frame, app);
    }

    if app.confirm.is_some() {
        draw_confirm_dialog(frame, app);
    }
}

fn draw_tab_bar(frame: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Span> = Tab::ALL
        .iter()
        .map(|t| {
            if *t == app.tab {
                Span::styled(
                    t.label(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(t.label(), Style::default().fg(Color::DarkGray))
            }
        })
        .collect();

    let tabs = Tabs::new(titles)
        .select(app.tab.index())
        .divider("│")
        .highlight_style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let line = if let Some(ref err) = app.error {
        Line::from(vec![Span::styled(
            err.as_str(),
            Style::default().fg(Color::Red),
        )])
    } else if app.create_task_prompt.is_some() {
        Line::from(vec![Span::styled(
            " j/k: choose project | Enter: open editor | q/Esc: cancel",
            Style::default().add_modifier(Modifier::DIM),
        )])
    } else {
        let hints = match app.detail {
            Some(DetailView::TaskLog { .. } | DetailView::EnvironmentLog { .. }) => {
                " q/Esc: back | j/k: scroll | g/G: top/bottom | d/u: half-page"
            }
            None => match app.tab {
                Tab::Tasks => match app.task_view_mode {
                    TaskViewMode::Flat => {
                        " Tab: tabs | j/k: navigate | Enter: logs | n: new | d: delete | D: force delete | `: flat/tree | q: quit"
                    }
                    TaskViewMode::Tree => {
                        " Tab: tabs | j/k: navigate | h/l: collapse/expand | Enter: logs | n: new | d: delete | D: force delete | `: flat/tree | q: quit"
                    }
                },
                Tab::Projects => " Tab: tabs | j/k: navigate | d/D: delete | q: quit",
                Tab::Environments => {
                    " Tab: tabs | j/k: navigate | Enter: logs | d: delete | D: force delete | q: quit"
                }
                Tab::Daemon => " Tab: tabs | q: quit",
                Tab::Logs => {
                    " Tab: tabs | j/k: scroll | g/G: top/bottom | d/u: half-page | q: quit"
                }
            },
        };
        Line::from(vec![Span::styled(
            hints,
            Style::default().add_modifier(Modifier::DIM),
        )])
    };

    frame.render_widget(Paragraph::new(line), area);
}

fn status_span(status: &str, tick_count: usize) -> Span<'static> {
    match status {
        "pending" => Span::styled(format!("● {status}"), Style::default().fg(Color::Yellow)),
        "started" => {
            let spinner = SPINNER_FRAMES[tick_count % SPINNER_FRAMES.len()];
            Span::styled(
                format!("{spinner} {status}"),
                Style::default().fg(Color::Blue),
            )
        }
        "complete" => Span::styled(format!("✓ {status}"), Style::default().fg(Color::Green)),
        "failed" => Span::styled(format!("✗ {status}"), Style::default().fg(Color::Red)),
        _ => Span::raw(status.to_string()),
    }
}

fn short_id(id: &str) -> &str {
    if id.len() > 8 { &id[..8] } else { id }
}

fn scroll_top_for_bottom_follow(line_index: usize, area: Rect) -> u16 {
    // Paragraph text area excludes top/bottom borders.
    let visible_lines = area.height.saturating_sub(2) as usize;
    let top = if visible_lines > 0 {
        line_index.saturating_sub(visible_lines.saturating_sub(1))
    } else {
        line_index
    };
    top.min(u16::MAX as usize) as u16
}

fn row_style(selected: bool) -> Style {
    if selected {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    }
}

fn draw_task_list_flat(frame: &mut Frame, app: &App, tick_count: usize, area: Rect) {
    let header = Row::new(["TASK", "PROJECT", "STATUS", "DESCRIPTION"])
        .style(Style::default().add_modifier(Modifier::BOLD | Modifier::DIM));

    let rows: Vec<Row> = app
        .tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let project = app.project_name(&task.project_id);
            let status = status_span(&task.status, tick_count);

            Row::new(vec![
                Cell::from(short_id(&task.id).to_string()),
                Cell::from(project.to_string()),
                Cell::from(status),
                Cell::from(task.description.clone()),
            ])
            .style(row_style(i == app.selected))
        })
        .collect();

    let widths = [
        Constraint::Length(10),
        Constraint::Length(14),
        Constraint::Length(12),
        Constraint::Fill(1),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL));

    frame.render_widget(table, area);
}

fn draw_task_list_tree(frame: &mut Frame, app: &App, tick_count: usize, area: Rect) {
    let rows: Vec<Row> = app
        .tree_rows
        .iter()
        .enumerate()
        .map(|(i, tree_row)| {
            let style = row_style(i == app.selected);
            match tree_row {
                TreeRow::Project(pi) => {
                    let name = &app.projects[*pi].name;
                    let arrow = if app.is_project_collapsed(*pi) {
                        "▶ "
                    } else {
                        "▼ "
                    };
                    Row::new(vec![Cell::from(Line::from(vec![
                        Span::styled(arrow, Style::default().fg(Color::DarkGray)),
                        Span::styled(name.clone(), Style::default().add_modifier(Modifier::BOLD)),
                    ]))])
                    .style(style)
                }
                TreeRow::Task(ti) => {
                    let task = &app.tasks[*ti];
                    let status = status_span(&task.status, tick_count);
                    let prefix = if app.is_task_collapsed(*ti) {
                        "  ├▶"
                    } else {
                        "  ├▼"
                    };
                    Row::new(vec![Cell::from(Line::from(vec![
                        Span::styled(prefix, Style::default().fg(Color::DarkGray)),
                        Span::raw(format!("{} ", short_id(&task.id))),
                        status,
                        Span::raw(format!("  {}", task.description)),
                    ]))])
                    .style(style)
                }
                TreeRow::TaskEnvironment(ti) => {
                    let task = &app.tasks[*ti];
                    let (env_id_str, env_status) =
                        if let Some(env) = app.find_environment(&task.environment_id) {
                            (
                                short_id(&env.id).to_string(),
                                status_span(&env.status, tick_count),
                            )
                        } else {
                            (
                                short_id(&task.environment_id).to_string(),
                                Span::styled("?", Style::default().fg(Color::DarkGray)),
                            )
                        };
                    Row::new(vec![Cell::from(Line::from(vec![
                        Span::styled("  │ └ ", Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            format!("env {env_id_str} "),
                            Style::default().fg(Color::DarkGray),
                        ),
                        env_status,
                    ]))])
                    .style(style)
                }
            }
        })
        .collect();

    let widths = [Constraint::Fill(1)];

    let table = Table::new(rows, widths).block(Block::default().borders(Borders::ALL));

    frame.render_widget(table, area);
}

fn draw_project_list(frame: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(["NAME", "PATH"])
        .style(Style::default().add_modifier(Modifier::BOLD | Modifier::DIM));

    let rows: Vec<Row> = app
        .projects
        .iter()
        .enumerate()
        .map(|(i, project)| {
            Row::new(vec![
                Cell::from(project.name.clone()),
                Cell::from(project.path.clone()),
            ])
            .style(row_style(i == app.selected))
        })
        .collect();

    let widths = [Constraint::Length(20), Constraint::Fill(1)];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL));

    frame.render_widget(table, area);
}

fn draw_environment_list(frame: &mut Frame, app: &App, tick_count: usize, area: Rect) {
    let header = Row::new(["ID", "PROJECT", "PROVIDER", "STATUS"])
        .style(Style::default().add_modifier(Modifier::BOLD | Modifier::DIM));

    let rows: Vec<Row> = app
        .environments
        .iter()
        .enumerate()
        .map(|(i, env)| {
            let project = app.project_name(&env.project_id);
            let status = status_span(&env.status, tick_count);

            Row::new(vec![
                Cell::from(short_id(&env.id).to_string()),
                Cell::from(project.to_string()),
                Cell::from(env.provider.clone()),
                Cell::from(status),
            ])
            .style(row_style(i == app.selected))
        })
        .collect();

    let widths = [
        Constraint::Length(10),
        Constraint::Length(14),
        Constraint::Length(14),
        Constraint::Fill(1),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL));

    frame.render_widget(table, area);
}

fn draw_daemon_view(frame: &mut Frame, app: &App, tick_count: usize, area: Rect) {
    let status = if app.daemon_connected {
        let spinner = SPINNER_FRAMES[tick_count % SPINNER_FRAMES.len()];
        Line::from(vec![Span::styled(
            format!(" {spinner} connected"),
            Style::default().fg(Color::Green),
        )])
    } else {
        Line::from(vec![Span::styled(
            " ✗ disconnected",
            Style::default().fg(Color::Red),
        )])
    };

    let runtime_dir = crate::paths::runtime_dir().ok();
    let socket_path = runtime_dir
        .as_ref()
        .map(|d| d.join("work.sock").display().to_string())
        .unwrap_or_else(|| "-".to_string());
    let pid = runtime_dir
        .as_ref()
        .and_then(|d| std::fs::read_to_string(d.join("work.pid")).ok())
        .unwrap_or_else(|| "-".to_string());

    let lines = vec![
        Line::default(),
        status,
        Line::default(),
        Line::from(vec![
            Span::styled(" pid:    ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(pid.trim().to_string()),
        ]),
        Line::from(vec![
            Span::styled(" socket: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(socket_path),
        ]),
        Line::from(vec![
            Span::styled(" tasks:  ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(app.tasks.len().to_string()),
        ]),
        Line::from(vec![
            Span::styled(" envs:   ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(app.environments.len().to_string()),
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn draw_log_view(frame: &mut Frame, app: &App, area: Rect) {
    let title = match app.detail.as_ref() {
        Some(DetailView::TaskLog { task_id }) => {
            let desc = app
                .tasks
                .iter()
                .find(|t| t.id == *task_id)
                .map(|t| t.description.as_str())
                .unwrap_or("");
            format!(" task {} - {desc} ", short_id(task_id))
        }
        Some(DetailView::EnvironmentLog { env_id }) => {
            let provider = app
                .environments
                .iter()
                .find(|e| e.id == *env_id)
                .map(|e| e.provider.as_str())
                .unwrap_or("-");
            format!(" env {} - {provider} ", short_id(env_id))
        }
        None => " logs ".to_string(),
    };

    let log = Paragraph::new(app.log_content.as_str())
        .block(Block::default().borders(Borders::ALL).title(title))
        .scroll((scroll_top_for_bottom_follow(app.log_scroll, area), 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(log, area);
}

fn draw_tui_logs_view(frame: &mut Frame, app: &App, area: Rect) {
    let log = Paragraph::new(app.tui_log_content.as_str())
        .block(Block::default().borders(Borders::ALL).title(" TUI Logs "))
        .scroll((scroll_top_for_bottom_follow(app.tui_log_scroll, area), 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(log, area);
}

fn draw_create_task_prompt(frame: &mut Frame, app: &App) {
    let Some(prompt) = app.create_task_prompt.as_ref() else {
        return;
    };
    if app.projects.is_empty() {
        return;
    }

    let selected = prompt
        .selected_project
        .min(app.projects.len().saturating_sub(1));
    let max_visible = 6usize;
    let mut start = selected.saturating_sub(max_visible / 2);
    let mut end = (start + max_visible).min(app.projects.len());
    start = end.saturating_sub(max_visible);
    end = end.max(start);

    let mut lines = vec![
        Line::from(vec![Span::styled(
            "Select project",
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            "A new task prompt opens in $EDITOR after confirmation.",
            Style::default().fg(Color::DarkGray),
        )]),
        Line::default(),
    ];

    if start > 0 {
        lines.push(Line::from(vec![Span::styled(
            format!("  … {start} more above"),
            Style::default().fg(Color::DarkGray),
        )]));
    }

    for (idx, project) in app
        .projects
        .iter()
        .enumerate()
        .skip(start)
        .take(end - start)
    {
        let is_selected = idx == selected;
        let marker = if is_selected { "›" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {marker} "),
                Style::default().fg(if is_selected {
                    Color::LightCyan
                } else {
                    Color::DarkGray
                }),
            ),
            Span::styled(
                project.name.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(if is_selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::styled(
                format!("  {}", project.path),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    if end < app.projects.len() {
        lines.push(Line::from(vec![Span::styled(
            format!("  … {} more below", app.projects.len() - end),
            Style::default().fg(Color::DarkGray),
        )]));
    }

    lines.push(Line::default());
    lines.push(Line::from(vec![Span::styled(
        "Enter: confirm and open editor    q/Esc: cancel",
        Style::default().fg(Color::Gray),
    )]));

    let content_height = lines.len() as u16 + 2;
    let area = centered_rect(86, content_height, frame.area());
    frame.render_widget(Clear, area);

    let dialog = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" New Task ")
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(dialog, area);
}

fn draw_confirm_dialog(frame: &mut Frame, app: &App) {
    let (target_label, target_value, skip_provider) = match app.confirm {
        Some(Confirm::Task {
            ref task_id,
            skip_provider,
        }) => ("Task", short_id(task_id).to_string(), skip_provider),
        Some(Confirm::Project { ref project_name }) => ("Project", project_name.clone(), false),
        Some(Confirm::Environment {
            ref env_id,
            skip_provider,
        }) => ("Environment", short_id(env_id).to_string(), skip_provider),
        None => {
            return;
        }
    };

    let area = if skip_provider {
        centered_rect(70, 11, frame.area())
    } else {
        centered_rect(62, 9, frame.area())
    };
    frame.render_widget(Clear, area);

    let mut body = vec![
        Line::from(vec![Span::styled(
            if skip_provider {
                format!("Force-delete {target_label}?")
            } else {
                format!("Delete {target_label}?")
            },
            Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::default(),
        Line::from(vec![
            Span::styled(
                format!("{target_label}: "),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                target_value,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    if skip_provider {
        body.push(Line::default());
        body.push(Line::from(vec![Span::styled(
            "Provider cleanup will be skipped (database records only).",
            Style::default().fg(Color::Yellow),
        )]));
    }

    body.push(Line::default());
    body.push(Line::from(vec![Span::styled(
        "Press y to confirm, n or Esc to cancel.",
        Style::default().fg(Color::Gray),
    )]));

    let title = if skip_provider {
        " Confirm Force Delete "
    } else {
        " Confirm Delete "
    };

    let border_color = if skip_provider {
        Color::Yellow
    } else {
        Color::LightRed
    };

    let dialog = Paragraph::new(body).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(border_color)),
    );

    frame.render_widget(dialog, area);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
