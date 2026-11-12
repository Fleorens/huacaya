//! Applying one key press to [`App`].
//!
//! The rules are a priority list: the first matching block wins. Key releases (`KeyEventKind::Release`)
//! are ignored everywhere, so a press and its release count once (held keys still repeat).

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::{App, INTERVAL_STEPS, PAGE, Screen};

/// Applies one key press to the state. Key releases (`KeyEventKind::Release`) are ignored.
pub fn handle(app: &mut App, key: KeyEvent) {
    // A release never counts, anywhere.
    if key.kind == KeyEventKind::Release {
        return;
    }

    // 1. Ctrl-C quits, from anywhere.
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.quit = true;
        return;
    }

    // 2. The filter is being typed after `/`.
    if app.log_view.editing_filter {
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.log_view.filter.push(c);
                app.log_view.scroll = 0;
                app.log_view.follow = true;
            }
            KeyCode::Backspace => {
                app.log_view.filter.pop();
                app.log_view.scroll = 0;
                app.log_view.follow = true;
            }
            KeyCode::Enter => app.log_view.editing_filter = false,
            KeyCode::Esc => {
                app.log_view.editing_filter = false;
                app.log_view.filter.clear();
                app.log_view.scroll = 0;
                app.log_view.follow = true;
            }
            _ => {}
        }
        return;
    }

    // 3. The help overlay is up.
    if app.help {
        match key.code {
            KeyCode::Char('?') | KeyCode::Esc => app.help = false,
            KeyCode::Char('q') => app.quit = true,
            _ => {}
        }
        return;
    }

    // 4. Everywhere.
    let handled = match key.code {
        KeyCode::Char('q') => {
            app.quit = true;
            true
        }
        KeyCode::Char('1') => {
            app.screen = Screen::Overview;
            true
        }
        KeyCode::Char('2') => {
            app.screen = Screen::Models;
            true
        }
        KeyCode::Char('3') => {
            app.screen = Screen::Logs;
            true
        }
        KeyCode::Char('4') => {
            app.screen = Screen::System;
            true
        }
        KeyCode::Tab => {
            app.screen = app.screen.next();
            true
        }
        KeyCode::BackTab => {
            app.screen = app.screen.prev();
            true
        }
        KeyCode::Char('?') => {
            app.help = true;
            true
        }
        KeyCode::Char('p') => {
            app.paused = !app.paused;
            true
        }
        KeyCode::Char('+') => {
            app.interval_ms = interval_below(app.interval_ms);
            true
        }
        KeyCode::Char('-') => {
            app.interval_ms = interval_above(app.interval_ms);
            true
        }
        _ => false,
    };
    if handled {
        return;
    }

    // 5. The Logs screen: scroll, follow, filters.
    if app.screen == Screen::Logs {
        // Compute the cap before mutating `log_view`, so the borrow checker is happy.
        let max = app.visible_logs().len().saturating_sub(1);
        match key.code {
            KeyCode::Up | KeyCode::PageUp => {
                let step = if key.code == KeyCode::PageUp { PAGE } else { 1 };
                app.log_view.follow = false;
                app.log_view.scroll = (app.log_view.scroll + step).min(max);
            }
            KeyCode::Down | KeyCode::PageDown => {
                let step = if key.code == KeyCode::PageDown { PAGE } else { 1 };
                app.log_view.scroll = app.log_view.scroll.saturating_sub(step);
                app.log_view.follow = app.log_view.scroll == 0;
            }
            KeyCode::Home => {
                app.log_view.scroll = max;
                app.log_view.follow = false;
            }
            KeyCode::End => {
                app.log_view.scroll = 0;
                app.log_view.follow = true;
            }
            KeyCode::Char('f') => {
                app.log_view.follow = !app.log_view.follow;
                if app.log_view.follow {
                    app.log_view.scroll = 0;
                }
            }
            KeyCode::Char('/') => app.log_view.editing_filter = true,
            KeyCode::Char('e') => {
                app.log_view.level = app.log_view.level.next();
                app.log_view.scroll = 0;
                app.log_view.follow = true;
            }
            KeyCode::Char('x') => {
                app.log_view.show_proxy = !app.log_view.show_proxy;
                app.log_view.scroll = 0;
                app.log_view.follow = true;
            }
            KeyCode::Esc => {
                app.log_view.filter.clear();
                app.log_view.scroll = 0;
                app.log_view.follow = true;
            }
            _ => {}
        }
        return;
    }

    // 6. The Models screen: move the selection within the models.
    if app.screen == Screen::Models {
        let n = app.llama.as_ref().map_or(0, |l| l.models.len());
        match key.code {
            KeyCode::Down => {
                if n > 0 {
                    app.model_selected = (app.model_selected + 1).min(n - 1);
                }
            }
            KeyCode::Up => {
                if n > 0 {
                    app.model_selected = app.model_selected.saturating_sub(1);
                }
            }
            KeyCode::Home => app.model_selected = 0,
            KeyCode::End if n > 0 => app.model_selected = n - 1,
            _ => {}
        }
    }
}

/// Largest `INTERVAL_STEPS` value strictly below `ms`; `ms` unchanged when there is none.
fn interval_below(ms: u64) -> u64 {
    INTERVAL_STEPS.iter().rev().find(|&&x| x < ms).copied().unwrap_or(ms)
}

/// Smallest `INTERVAL_STEPS` value strictly above `ms`; `ms` unchanged when there is none.
fn interval_above(ms: u64) -> u64 {
    INTERVAL_STEPS.iter().find(|&&x| x > ms).copied().unwrap_or(ms)
}
