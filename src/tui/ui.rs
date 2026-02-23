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
        Some(DetailView::TaskLog { .. }) => draw_log_view(frame, app, chunks[1]),
        None => match app.tab {
            Tab::Tasks => match app.task_view_mode {
                TaskViewMode::Flat => draw_task_list_flat(frame, app, tick_count, chunks[1]),
                TaskViewMode::Tree => draw_task_list_tree(frame, app, tick_count, chunks[1]),
            },
            Tab::Projects => draw_project_list(frame, app, chunks[1]),
            Tab::Environments => draw_environment_list(frame, app, tick_count, chunks[1]),
            Tab::Daemon => draw_daemon_view(frame, app, tick_count, chunks[1]),
        },
    }

    draw_status_bar(frame, app, chunks[2]);

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
    } else {
        let hints = match app.detail {
            Some(DetailView::TaskLog { .. }) => {
                " q/Esc: back | j/k: scroll | g/G: top/bottom | d/u: half-page"
            }
            None => match app.tab {
                Tab::Tasks => match app.task_view_mode {
                    TaskViewMode::Flat => {
                        " Tab: tabs | j/k: navigate | Enter: logs | D: delete | `: flat/tree | q: quit"
                    }
                    TaskViewMode::Tree => {
                        " Tab: tabs | j/k: navigate | h/l: collapse/expand | Enter: logs | D: delete | `: flat/tree | q: quit"
                    }
                },
                Tab::Projects => " Tab: tabs | j/k: navigate | D: delete | q: quit",
                Tab::Environments => " Tab: tabs | j/k: navigate | q: quit",
                Tab::Daemon => " Tab: tabs | q: quit",
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
                    let prefix = if task.environment_id.is_some() {
                        if app.is_task_collapsed(*ti) {
                            "  ├▶"
                        } else {
                            "  ├▼"
                        }
                    } else {
                        "  ├ "
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
                    let (env_id_str, env_status) = if let Some(ref eid) = task.environment_id {
                        if let Some(env) = app.find_environment(eid) {
                            (
                                short_id(&env.id).to_string(),
                                status_span(&env.status, tick_count),
                            )
                        } else {
                            (
                                short_id(eid).to_string(),
                                Span::styled("?", Style::default().fg(Color::DarkGray)),
                            )
                        }
                    } else {
                        return Row::new(vec![Cell::from("")]);
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
    let title = if let Some(DetailView::TaskLog { ref task_id }) = app.detail {
        let desc = app
            .tasks
            .iter()
            .find(|t| t.id == *task_id)
            .map(|t| t.description.as_str())
            .unwrap_or("");
        format!(" {} - {desc} ", short_id(task_id))
    } else {
        " logs ".to_string()
    };

    let log = Paragraph::new(app.log_content.as_str())
        .block(Block::default().borders(Borders::ALL).title(title))
        .scroll((app.log_scroll as u16, 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(log, area);
}

fn draw_confirm_dialog(frame: &mut Frame, app: &App) {
    let text = match app.confirm {
        Some(Confirm::DeleteTask { ref task_id }) => {
            format!("Delete task {}? (y/n)", short_id(task_id))
        }
        Some(Confirm::DeleteProject { ref project_name }) => {
            format!("Delete project {project_name}? (y/n)")
        }
        None => return,
    };

    let area = centered_rect(40, 5, frame.area());
    frame.render_widget(Clear, area);

    let dialog = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Confirm "))
        .style(Style::default().fg(Color::Yellow));

    frame.render_widget(dialog, area);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
