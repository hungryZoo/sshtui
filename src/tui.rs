//! Terminal initialization and restoration, and the event loop.

use std::io::{self, stdout};
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossterm::cursor::Show;
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, MouseButton, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::app::{App, Backend, ClickTarget, Exit};
use crate::ui::{self, HitMap};

// The terminal and panic hook both belong to the process.
static TERMINAL_SESSION: Mutex<()> = Mutex::new(());

/// Terminal features to enable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Capture mouse events for the TUI.
    pub mouse: bool,
}

/// Detects double clicks using caller-supplied times.
#[derive(Debug, Default)]
pub struct ClickTracker {
    pressed: Option<ClickTarget>,
    last: Option<(ClickTarget, u16, u16, Instant)>,
}

impl ClickTracker {
    /// Records a click, returning true for the same target and cell within 400 ms.
    /// A double click resets the tracker so a third click starts a new pair.
    pub fn register(&mut self, target: &ClickTarget, column: u16, row: u16, now: Instant) -> bool {
        let double = self.last.as_ref().is_some_and(|(last, x, y, time)| {
            last == target
                && (*x, *y) == (column, row)
                && now
                    .checked_duration_since(*time)
                    .is_some_and(|elapsed| elapsed <= Duration::from_millis(400))
        });
        self.last = if double {
            None
        } else {
            Some((target.clone(), column, row, now))
        };
        double
    }
}

/// Runs the event loop until the application requests an exit. The terminal is restored
/// before this function returns, on success, error and panic alike.
pub fn run<B: Backend>(app: App<B>, options: Options) -> io::Result<Exit> {
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
    let result = panic::catch_unwind(AssertUnwindSafe(|| run_loop(app, options)));
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

fn run_loop<B: Backend>(mut app: App<B>, options: Options) -> io::Result<Exit> {
    // Drop the session guard before ratatui restores the cursor.
    let mut terminal;
    let mut session = RestoreTerminal { restored: false };
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
    if options.mouse {
        execute!(stdout(), EnableMouseCapture)?;
    }
    terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut clicks = ClickTracker::default();

    let exit = loop {
        if let Some(exit) = app.exit() {
            break exit.clone();
        }
        let mut hits = HitMap::default();
        terminal.draw(|frame| hits = ui::render(frame, &app))?;
        loop {
            let mouse = options.mouse.then_some((&hits, &mut clicks));
            if handle_event(&mut app, event::read()?, mouse, Instant::now()) {
                break;
            }
        }
        if app.take_full_redraw() {
            terminal.clear()?;
        }
    };

    if options.mouse {
        // Stop new mouse reports and discard queued input while raw mode is still active.
        execute!(stdout(), DisableMouseCapture)?;
        let deadline = Instant::now() + Duration::from_millis(250);
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            if !event::poll(remaining.min(Duration::from_millis(50)))? {
                break;
            }
            event::read()?;
        }
    }
    restore_terminal()?;
    session.restored = true;
    Ok(exit)
}

fn handle_event<B: Backend>(
    app: &mut App<B>,
    event: Event,
    mouse_context: Option<(&HitMap, &mut ClickTracker)>,
    now: Instant,
) -> bool {
    match event {
        Event::Key(key) => app.handle_key(key),
        Event::Paste(text) => app.handle_paste(&text),
        Event::Resize(..) => {}
        Event::Mouse(mouse) => {
            let Some((hits, clicks)) = mouse_context else {
                return false;
            };
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    clicks.pressed = hits.hit(mouse.column, mouse.row).cloned();
                    return false;
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    let Some(target) = clicks
                        .pressed
                        .take()
                        .filter(|target| hits.hit(mouse.column, mouse.row) == Some(target))
                    else {
                        return false;
                    };
                    let double = clicks.register(&target, mouse.column, mouse.row, now);
                    app.handle_click(target, double);
                }
                MouseEventKind::ScrollUp => app.handle_scroll(false),
                MouseEventKind::ScrollDown => app.handle_scroll(true),
                _ => return false,
            }
        }
        _ => return false,
    }
    true
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
    let mouse = execute!(stdout(), DisableMouseCapture);
    let paste = execute!(stdout(), DisableBracketedPaste);
    let screen = execute!(stdout(), LeaveAlternateScreen);
    let raw = disable_raw_mode();
    let cursor = execute!(stdout(), Show);
    mouse.and(paste).and(screen).and(raw).and(cursor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{FormField, Mode};
    use crate::ui::tests::FakeBackend;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
    use ratatui::backend::TestBackend;

    const SECOND_HOST: (u16, u16) = (12, 3);
    const LEFT_DOWN: MouseEventKind = MouseEventKind::Down(MouseButton::Left);
    const LEFT_UP: MouseEventKind = MouseEventKind::Up(MouseButton::Left);

    fn rendered_hits(app: &App<FakeBackend>, height: u16) -> HitMap {
        let mut terminal = Terminal::new(TestBackend::new(80, height)).unwrap();
        let mut hits = HitMap::default();
        terminal
            .draw(|frame| hits = ui::render(frame, app))
            .unwrap();
        hits
    }

    fn mouse_event(kind: MouseEventKind, (column, row): (u16, u16)) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn dispatches_keys_and_paste_and_ignores_other_events() {
        let mut app = App::new(FakeBackend::new("Host web\nHost db\n"));
        let now = Instant::now();
        handle_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            None,
            now,
        );
        assert_eq!(app.selection(), Some(1));
        handle_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)),
            None,
            now,
        );
        assert!(matches!(app.mode(), Mode::Form(_)));
        handle_event(&mut app, Event::Paste("new-host".into()), None, now);

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
            handle_event(&mut app, event, None, now);
        }
        let form = app.form().unwrap();
        assert_eq!(form.focus(), FormField::Nickname);
        assert_eq!(form.input(FormField::Nickname).value(), "new-host");
        assert_eq!(app.selection(), Some(1));
        assert!(app.exit().is_none());
        assert!(!app.take_full_redraw());
    }

    #[test]
    fn double_clicks_include_the_deadline_and_reset_the_pair() {
        let now = Instant::now();
        let target = ClickTarget::HostRow(1);
        let mut clicks = ClickTracker::default();
        assert!(!clicks.register(&target, 3, 5, now));
        assert!(clicks.register(&target, 3, 5, now + Duration::from_millis(400)));
        assert!(!clicks.register(&target, 3, 5, now + Duration::from_millis(401)));
        assert!(clicks.register(&target, 3, 5, now + Duration::from_millis(402)));
    }

    #[test]
    fn double_clicks_require_the_same_target_and_cell_within_the_deadline() {
        let now = Instant::now();
        let target = ClickTarget::HostRow(1);
        for (next, column, row, millis) in [
            (&target, 3, 5, 401),
            (&target, 4, 5, 100),
            (&target, 3, 6, 100),
            (&ClickTarget::HostRow(2), 3, 5, 100),
        ] {
            let mut clicks = ClickTracker::default();
            assert!(!clicks.register(&target, 3, 5, now));
            assert!(!clicks.register(next, column, row, now + Duration::from_millis(millis)));
            assert!(clicks.register(next, column, row, now + Duration::from_millis(millis + 1)));
        }
    }

    #[test]
    fn mouse_dispatch_uses_the_rendered_hits_and_skips_redraw_for_ignored_events() {
        let mut app = App::new(FakeBackend::new("Host web\nHost db\n"));
        let hits = rendered_hits(&app, 10);
        let mut clicks = ClickTracker::default();
        let now = Instant::now();
        for kind in [
            MouseEventKind::Moved,
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::ScrollLeft,
        ] {
            assert!(!handle_event(
                &mut app,
                mouse_event(kind, SECOND_HOST),
                Some((&hits, &mut clicks)),
                now,
            ));
        }
        for kind in [LEFT_DOWN, LEFT_UP] {
            assert!(!handle_event(
                &mut app,
                mouse_event(kind, (0, SECOND_HOST.1)),
                Some((&hits, &mut clicks)),
                now,
            ));
            assert!(!handle_event(
                &mut app,
                mouse_event(kind, SECOND_HOST),
                None,
                now
            ));
        }
        assert_eq!(app.selection(), Some(0));

        for (kind, selection) in [
            (MouseEventKind::ScrollDown, 1),
            (MouseEventKind::ScrollUp, 0),
        ] {
            assert!(handle_event(
                &mut app,
                mouse_event(kind, SECOND_HOST),
                Some((&hits, &mut clicks)),
                now,
            ));
            assert_eq!(app.selection(), Some(selection));
        }
        assert!(!handle_event(
            &mut app,
            mouse_event(LEFT_DOWN, SECOND_HOST),
            Some((&hits, &mut clicks)),
            now,
        ));
        assert!(handle_event(
            &mut app,
            mouse_event(LEFT_UP, SECOND_HOST),
            Some((&hits, &mut clicks)),
            now,
        ));
        assert_eq!(app.selection(), Some(1));
        assert_eq!(app.exit(), None);
    }

    #[test]
    fn left_press_does_not_change_selection_or_request_redraw() {
        let mut app = App::new(FakeBackend::new("Host web\nHost db\n"));
        let hits = rendered_hits(&app, 10);
        let mut clicks = ClickTracker::default();
        assert!(!handle_event(
            &mut app,
            mouse_event(LEFT_DOWN, SECOND_HOST),
            Some((&hits, &mut clicks)),
            Instant::now(),
        ));
        assert_eq!(app.selection(), Some(0));
        assert_eq!(app.exit(), None);
    }

    #[test]
    fn left_release_on_the_pressed_host_row_selects_it() {
        let mut app = App::new(FakeBackend::new("Host web\nHost db\n"));
        let hits = rendered_hits(&app, 10);
        let mut clicks = ClickTracker::default();
        let now = Instant::now();
        handle_event(
            &mut app,
            mouse_event(LEFT_DOWN, SECOND_HOST),
            Some((&hits, &mut clicks)),
            now,
        );
        assert!(handle_event(
            &mut app,
            mouse_event(LEFT_UP, (SECOND_HOST.0 + 1, SECOND_HOST.1)),
            Some((&hits, &mut clicks)),
            now,
        ));
        assert_eq!(app.selection(), Some(1));
        assert_eq!(app.exit(), None);
    }

    #[test]
    fn releasing_off_the_pressed_button_clears_the_press_without_acting() {
        let mut app = App::new(FakeBackend::new(""));
        let hits = rendered_hits(&app, 24);
        let button = (0..24)
            .flat_map(|row| (0..80).map(move |column| (column, row)))
            .find(|&(column, row)| hits.hit(column, row) == Some(&ClickTarget::Key(KeyCode::F(2))))
            .unwrap();
        let field = (12, 4);
        assert!(matches!(
            hits.hit(field.0, field.1),
            Some(ClickTarget::FormField {
                field: FormField::Address,
                ..
            })
        ));

        let mut clicks = ClickTracker::default();
        let now = Instant::now();
        for (kind, cell) in [(LEFT_DOWN, button), (LEFT_UP, field), (LEFT_UP, button)] {
            assert!(!handle_event(
                &mut app,
                mouse_event(kind, cell),
                Some((&hits, &mut clicks)),
                now,
            ));
            assert_eq!(app.form().unwrap().focus(), FormField::Nickname);
            assert_eq!(app.exit(), None);
        }
    }

    #[test]
    fn left_release_without_a_press_is_ignored() {
        let mut app = App::new(FakeBackend::new("Host web\nHost db\n"));
        let hits = rendered_hits(&app, 10);
        let mut clicks = ClickTracker::default();
        assert!(!handle_event(
            &mut app,
            mouse_event(LEFT_UP, SECOND_HOST),
            Some((&hits, &mut clicks)),
            Instant::now(),
        ));
        assert_eq!(app.selection(), Some(0));
        assert_eq!(app.exit(), None);
    }

    #[test]
    fn double_click_connects_on_the_second_release_using_release_times() {
        let mut app = App::new(FakeBackend::new("Host web\nHost db\n"));
        let mut hits = rendered_hits(&app, 10);
        let mut clicks = ClickTracker::default();
        let now = Instant::now();
        let connect = Exit::Connect(vec!["db".into()]);
        for (kind, ms, redraw, exit) in [
            (LEFT_DOWN, 0, false, None),
            (LEFT_UP, 100, true, None),
            (LEFT_DOWN, 450, false, None),
            (LEFT_UP, 500, true, Some(&connect)),
        ] {
            assert_eq!(
                handle_event(
                    &mut app,
                    mouse_event(kind, SECOND_HOST),
                    Some((&hits, &mut clicks)),
                    now + Duration::from_millis(ms),
                ),
                redraw,
            );
            assert_eq!(app.exit(), exit);
            if redraw && exit.is_none() {
                hits = rendered_hits(&app, 10);
            }
        }
    }
}
