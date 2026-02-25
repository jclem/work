mod app;
mod ui;

use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::client::{DaemonClient, DaemonEvent};

use app::{App, Tab};

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
                    handle_key(&mut app, &client, key).await;
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

async fn handle_key(app: &mut App, client: &DaemonClient, key: event::KeyEvent) {
    // Confirm dialog takes priority.
    if app.confirm.is_some() {
        match key.code {
            KeyCode::Char('y') => app.confirm_delete(client).await,
            KeyCode::Char('n') | KeyCode::Esc => app.cancel_confirm(),
            _ => {}
        }
        return;
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
        return;
    }

    // Global keys (when no detail view is open).
    match key.code {
        KeyCode::Char('q') => {
            app.should_quit = true;
            return;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
            return;
        }
        KeyCode::Tab => {
            app.next_tab();
            return;
        }
        KeyCode::BackTab => {
            app.prev_tab();
            return;
        }
        KeyCode::Char('1') => {
            app.select_tab(0);
            return;
        }
        KeyCode::Char('2') => {
            app.select_tab(1);
            return;
        }
        KeyCode::Char('3') => {
            app.select_tab(2);
            return;
        }
        KeyCode::Char('4') => {
            app.select_tab(3);
            return;
        }
        KeyCode::Char('5') => {
            app.select_tab(4);
            return;
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
            KeyCode::Char('D') => app.prompt_delete(),
            KeyCode::Char('`') => app.toggle_task_view_mode(),
            _ => {}
        },
        Tab::Projects => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.select_next(),
            KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
            KeyCode::Char('D') => app.prompt_delete(),
            _ => {}
        },
        Tab::Environments => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.select_next(),
            KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
            KeyCode::Enter => app.enter_detail(),
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
}
