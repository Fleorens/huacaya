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
        KeyCode::Char('5') => {
            app.screen = Screen::Agents;
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
        KeyCode::Char('r') => {
            app.log_view.raw = !app.log_view.raw;
            true
        }
        _ => false,
    };
    if handled {
        return;
    }

    // 5. The Logs screen: scroll, follow, filters, tasks.
    if app.screen == Screen::Logs {
        // `t` toggles the tasks focus, from anywhere on the screen.
        if key.code == KeyCode::Char('t') {
            app.log_view.tasks_focus = !app.log_view.tasks_focus;
            return;
        }

        // While focused on the tasks list, the navigation keys and Enter move the selection or filter on
        // the task; every other key (Esc included) falls through to the log handling below.
        if app.log_view.tasks_focus {
            let tasks = crate::app::log_tasks::log_tasks(app.logs.iter(), 50);
            // The list changes as lines arrive or leave the buffer: keep the index inside it.
            app.log_view.task_selected = app.log_view.task_selected.min(tasks.len().saturating_sub(1));
            let handled = match key.code {
                KeyCode::Up => {
                    if !tasks.is_empty() {
                        app.log_view.task_selected = app.log_view.task_selected.saturating_sub(1);
                    }
                    true
                }
                KeyCode::Home => {
                    if !tasks.is_empty() {
                        app.log_view.task_selected = 0;
                    }
                    true
                }
                KeyCode::Down => {
                    if !tasks.is_empty() {
                        app.log_view.task_selected = (app.log_view.task_selected + 1).min(tasks.len() - 1);
                    }
                    true
                }
                KeyCode::End => {
                    if !tasks.is_empty() {
                        app.log_view.task_selected = tasks.len() - 1;
                    }
                    true
                }
                KeyCode::Enter => {
                    if let Some(summary) = tasks.get(app.log_view.task_selected) {
                        let task = summary.task;
                        app.log_view.task = if app.log_view.task == Some(task) { None } else { Some(task) };
                        app.log_view.scroll = 0;
                        app.log_view.follow = true;
                    }
                    true
                }
                _ => false,
            };
            if handled {
                return;
            }
        }

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
                // Esc (focused or not) clears the task filter too.
                app.log_view.task = None;
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

    // 7. The System screen: `s` toggles every sensor / the summary.
    if app.screen == Screen::System && key.code == KeyCode::Char('s') {
        app.all_sensors = !app.all_sensors;
    }

    // 8. The Agents screen: move the selection within the sessions, follow / unfollow.
    if app.screen == Screen::Agents {
        let n = app.opencode.as_ref().map_or(0, |o| o.sessions.len());
        match key.code {
            KeyCode::Down => {
                if n > 0 {
                    let i = (app.agent_selected + 1).min(n - 1);
                    app.agent_selected = i;
                    app.agent_focus = app.opencode.as_ref().map(|o| o.sessions[i].id.clone());
                }
            }
            KeyCode::Up => {
                if n > 0 {
                    let i = app.agent_selected.saturating_sub(1);
                    app.agent_selected = i;
                    app.agent_focus = app.opencode.as_ref().map(|o| o.sessions[i].id.clone());
                }
            }
            KeyCode::Home => {
                if n > 0 {
                    app.agent_selected = 0;
                    app.agent_focus = app.opencode.as_ref().map(|o| o.sessions[0].id.clone());
                }
            }
            KeyCode::End if n > 0 => {
                app.agent_selected = n - 1;
                app.agent_focus = app.opencode.as_ref().map(|o| o.sessions[n - 1].id.clone());
            }
            KeyCode::Char('a') => app.agent_focus = None,
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
