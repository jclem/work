mod app;
mod ui;

use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::client::{DaemonClient, DaemonEvent};
use crate::db::Project;

use app::{App, Tab};

enum EditorOutcome {
    Submitted(String),
    Cancelled(String),
}

pub async fn run(client: DaemonClient) -> anyhow::Result<()> {
    // Set panic hook to restore terminal on panic.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = terminal::disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));

    terminal::enable_raw_mode()?;
    crossterm::execute!(io::stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    app.poll(&client).await;

    let mut events_rx = client.subscribe_events();

    // Spawn a blocking task to read key events into a channel.
    // Use poll() with a timeout so the thread can notice when the
    // receiver is dropped and exit without waiting for a keypress.
    let (key_tx, mut key_rx) = tokio::sync::mpsc::channel(32);
    let input_handle = tokio::task::spawn_blocking(move || {
        loop {
            if !event::poll(std::time::Duration::from_millis(50)).unwrap_or(false) {
                if key_tx.is_closed() {
                    break;
                }
                continue;
            }
            match event::read() {
                Ok(Event::Key(key)) => {
                    if key_tx.blocking_send(key).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });

    let mut tick_count: usize = 0;
    let mut tick_interval = tokio::time::interval(std::time::Duration::from_millis(250));

    loop {
        terminal.draw(|frame| ui::draw(frame, &app, tick_count))?;

        tokio::select! {
            _ = tick_interval.tick() => {
                tick_count = tick_count.wrapping_add(1);
                if app.detail.is_some() {
                    app.refresh_detail_logs();
                }
                if app.tab == Tab::Logs {
                    app.refresh_tui_logs();
                }
            }
            result = events_rx.recv() => {
                match result {
                    Some(DaemonEvent::Connected | DaemonEvent::Updated) => {
                        // Drain any buffered events to avoid redundant polls.
                        while events_rx.try_recv().is_ok() {}
                        app.poll(&client).await;
                    }
                    Some(DaemonEvent::Disconnected) => {
                        app.set_disconnected();
                    }
                    None => {
                        // Channel closed; reconnect.
                        events_rx = client.subscribe_events();
                    }
                }
            }
            key = key_rx.recv() => {
                if let Some(key) = key {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    let needs_full_redraw = handle_key(&mut app, &client, key).await;
                    if needs_full_redraw {
                        terminal.clear()?;
                    }
                    if app.should_quit {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
    }

    // Drop the receiver so the input thread detects the closed channel and exits.
    drop(key_rx);
    // Restore terminal.
    terminal::disable_raw_mode()?;
    crossterm::execute!(io::stdout(), LeaveAlternateScreen)?;
    // Wait for the input thread to finish (it will exit within ~50ms).
    let _ = input_handle.await;

    Ok(())
}

async fn handle_key(app: &mut App, client: &DaemonClient, key: event::KeyEvent) -> bool {
    // Confirm dialog takes priority.
    if app.confirm.is_some() {
        match key.code {
            KeyCode::Char('y') => app.confirm_delete(client).await,
            KeyCode::Char('n') | KeyCode::Esc => app.cancel_confirm(),
            _ => {}
        }
        return false;
    }

    if app.create_task_prompt.is_some() {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.create_task_prompt_select_next(),
            KeyCode::Char('k') | KeyCode::Up => app.create_task_prompt_select_prev(),
            KeyCode::Enter => {
                confirm_create_task_prompt(app, client).await;
                return true;
            }
            KeyCode::Char('q') | KeyCode::Esc => app.cancel_create_task_prompt(),
            _ => {}
        }
        return false;
    }

    // Detail view (e.g. log view) takes priority over tab content.
    if app.detail.is_some() {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => app.exit_detail(),
            KeyCode::Char('j') | KeyCode::Down => app.scroll_log_down(1),
            KeyCode::Char('k') | KeyCode::Up => app.scroll_log_up(1),
            KeyCode::Char('g') => app.scroll_log_top(),
            KeyCode::Char('G') => app.scroll_log_bottom(),
            KeyCode::Char('d') => app.scroll_log_down(20),
            KeyCode::Char('u') => app.scroll_log_up(20),
            _ => {}
        }
        return false;
    }

    // Global keys (when no detail view is open).
    match key.code {
        KeyCode::Char('q') => {
            app.should_quit = true;
            return false;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
            return false;
        }
        KeyCode::Tab => {
            app.next_tab();
            return false;
        }
        KeyCode::BackTab => {
            app.prev_tab();
            return false;
        }
        KeyCode::Char('1') => {
            app.select_tab(0);
            return false;
        }
        KeyCode::Char('2') => {
            app.select_tab(1);
            return false;
        }
        KeyCode::Char('3') => {
            app.select_tab(2);
            return false;
        }
        KeyCode::Char('4') => {
            app.select_tab(3);
            return false;
        }
        KeyCode::Char('5') => {
            app.select_tab(4);
            return false;
        }
        _ => {}
    }

    // Tab-specific keys.
    match app.tab {
        Tab::Tasks => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.select_next(),
            KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
            KeyCode::Char('h') | KeyCode::Left => app.collapse_section(),
            KeyCode::Char('l') | KeyCode::Right => app.expand_section(),
            KeyCode::Char('H') => app.collapse_all(),
            KeyCode::Char('L') => app.expand_all(),
            KeyCode::Enter => app.enter_detail(),
            KeyCode::Char('d') => app.prompt_delete(),
            KeyCode::Char('D') => app.prompt_force_delete(),
            KeyCode::Char('n') => app.begin_create_task_prompt(),
            KeyCode::Char('`') => app.toggle_task_view_mode(),
            _ => {}
        },
        Tab::Projects => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.select_next(),
            KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
            KeyCode::Char('d') => app.prompt_delete(),
            KeyCode::Char('D') => app.prompt_force_delete(),
            _ => {}
        },
        Tab::Environments => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.select_next(),
            KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
            KeyCode::Enter => app.enter_detail(),
            KeyCode::Char('d') => app.prompt_delete(),
            KeyCode::Char('D') => app.prompt_force_delete(),
            _ => {}
        },
        Tab::Daemon => {}
        Tab::Logs => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.scroll_tui_log_down(1),
            KeyCode::Char('k') | KeyCode::Up => app.scroll_tui_log_up(1),
            KeyCode::Char('g') => app.scroll_tui_log_top(),
            KeyCode::Char('G') => app.scroll_tui_log_bottom(),
            KeyCode::Char('d') => app.scroll_tui_log_down(20),
            KeyCode::Char('u') => app.scroll_tui_log_up(20),
            _ => {}
        },
    }

    false
}

async fn confirm_create_task_prompt(app: &mut App, client: &DaemonClient) {
    let Some(project) = app.create_task_prompt_selected_project().cloned() else {
        app.cancel_create_task_prompt();
        app.error = Some("selected project is no longer available".to_string());
        return;
    };
    app.cancel_create_task_prompt();

    let description = match edit_task_description() {
        Ok(EditorOutcome::Submitted(description)) => description,
        Ok(EditorOutcome::Cancelled(reason)) => {
            app.error = Some(reason);
            return;
        }
        Err(e) => {
            app.error = Some(format!("task creation failed: {e}"));
            return;
        }
    };

    create_task_for_project(app, client, &project, &description).await;
}

async fn create_task_for_project(
    app: &mut App,
    client: &DaemonClient,
    project: &Project,
    description: &str,
) {
    let config = match crate::config::load() {
        Ok(config) => config,
        Err(e) => {
            app.error = Some(format!("task creation failed: {e}"));
            return;
        }
    };

    let Some(task_provider) = config.default_task_provider_for_project(&project.name) else {
        app.error = Some("--provider is required (or set task-provider in config)".to_string());
        return;
    };
    let Some(env_provider) = config.default_environment_provider_for_project(&project.name) else {
        app.error =
            Some("--env-provider is required (or set environment-provider in config)".to_string());
        return;
    };

    if let Err(e) = config.get_task_provider(&task_provider) {
        app.error = Some(format!("task creation failed: {e}"));
        return;
    }

    match client
        .create_task(&project.id, &task_provider, &env_provider, description)
        .await
    {
        Ok(_) => {
            app.error = None;
            app.poll(client).await;
        }
        Err(e) => {
            app.error = Some(format!("create failed: {e}"));
        }
    }
}

fn edit_task_description() -> anyhow::Result<EditorOutcome> {
    let editor = std::env::var("EDITOR").map_err(|_| anyhow::anyhow!("$EDITOR is not set"))?;
    let path = std::env::temp_dir().join(format!("work-task-{}.txt", crate::id::new_id()));
    std::fs::write(&path, "")?;

    terminal::disable_raw_mode()?;
    crossterm::execute!(io::stdout(), LeaveAlternateScreen)?;

    let status_result = std::process::Command::new(&editor).arg(&path).status();
    let restore_screen_result = crossterm::execute!(io::stdout(), EnterAlternateScreen);
    let restore_raw_result = terminal::enable_raw_mode();

    let contents = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::remove_file(&path);

    restore_screen_result?;
    restore_raw_result?;

    let status = status_result?;
    if !status.success() {
        return Ok(EditorOutcome::Cancelled(format!(
            "task creation cancelled ({editor} exited with {status})"
        )));
    }

    let description = contents.trim().to_string();
    if description.is_empty() {
        return Ok(EditorOutcome::Cancelled(
            "task description is empty".to_string(),
        ));
    }

    Ok(EditorOutcome::Submitted(description))
}
