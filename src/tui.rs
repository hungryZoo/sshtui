//! Terminal initialization and restoration, and the event loop.

use std::io::{self, stdout};
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

use crossterm::cursor::Show;
use crossterm::event::{self, DisableBracketedPaste, EnableBracketedPaste, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::app::{App, Backend, Exit};
use crate::ui;

// The terminal and panic hook both belong to the process.
static TERMINAL_SESSION: Mutex<()> = Mutex::new(());

/// Runs the event loop until the application requests an exit. The terminal is restored
/// before this function returns, on success, error and panic alike.
pub fn run<B: Backend>(app: App<B>) -> io::Result<Exit> {
    let session_lock = TERMINAL_SESSION
        .try_lock()
        .map_err(|_| io::Error::other("a terminal session is already active"))?;
    let previous_hook = Arc::new(panic::take_hook());
    let chained_hook = Arc::clone(&previous_hook);
    panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        chained_hook(info);
    }));

    // Restore the hook outside the unwind, since setting it from a panicking thread fails.
    let result = panic::catch_unwind(AssertUnwindSafe(|| run_loop(app)));
    drop(panic::take_hook());
    if let Some(hook) = Arc::into_inner(previous_hook) {
        panic::set_hook(hook);
    }
    drop(session_lock);

    match result {
        Ok(exit) => exit,
        Err(payload) => panic::resume_unwind(payload),
    }
}

fn run_loop<B: Backend>(mut app: App<B>) -> io::Result<Exit> {
    let mut session = RestoreTerminal { restored: false };
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let exit = loop {
        if let Some(exit) = app.exit() {
            break exit.clone();
        }
        terminal.draw(|frame| ui::render(frame, &app))?;
        handle_event(&mut app, event::read()?);
        if app.take_full_redraw() {
            terminal.clear()?;
        }
    };

    drop(terminal);
    restore_terminal()?;
    session.restored = true;
    Ok(exit)
}

fn handle_event<B: Backend>(app: &mut App<B>, event: Event) {
    match event {
        Event::Key(key) => app.handle_key(key),
        Event::Paste(text) => app.handle_paste(&text),
        _ => {}
    }
}

struct RestoreTerminal {
    restored: bool,
}

impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        if !self.restored {
            let _ = restore_terminal();
        }
    }
}

fn restore_terminal() -> io::Result<()> {
    // Attempt every operation even when a previous restoration step fails.
    let paste = execute!(stdout(), DisableBracketedPaste);
    let screen = execute!(stdout(), LeaveAlternateScreen);
    let raw = disable_raw_mode();
    let cursor = execute!(stdout(), Show);
    paste.and(screen).and(raw).and(cursor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{FormField, Mode};
    use crate::ui::tests::FakeBackend;
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };

    #[test]
    fn dispatches_keys_and_paste_and_ignores_other_events() {
        let mut app = App::new(FakeBackend::new("Host web\nHost db\n"));
        handle_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
        );
        assert_eq!(app.selection(), Some(1));
        handle_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)),
        );
        assert!(matches!(app.mode(), Mode::Form(_)));
        handle_event(&mut app, Event::Paste("new-host".into()));

        for event in [
            Event::Resize(20, 10),
            Event::FocusGained,
            Event::FocusLost,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 1,
                row: 1,
                modifiers: KeyModifiers::NONE,
            }),
        ] {
            handle_event(&mut app, event);
        }
        let form = app.form().unwrap();
        assert_eq!(form.focus(), FormField::Nickname);
        assert_eq!(form.input(FormField::Nickname).value(), "new-host");
        assert_eq!(app.selection(), Some(1));
        assert!(app.exit().is_none());
        assert!(!app.take_full_redraw());
    }
}
