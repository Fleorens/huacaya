//! The frozen view shown while the display is paused (`p`).
use super::state::App;
use std::borrow::Cow;

impl App {
    /// The state to draw: the data as of the moment `p` was pressed while paused, with the live UI
    /// state (screen, selection, filters...) so the frozen data stays navigable; `self` otherwise.
    pub fn shown(&self) -> Cow<'_, App> {
        let (true, Some(frozen)) = (self.paused, self.frozen.as_ref()) else {
            return Cow::Borrowed(self);
        };
        let mut view = (**frozen).clone();
        view.screen = self.screen;
        view.help = self.help;
        view.paused = self.paused;
        view.interval_ms = self.interval_ms;
        view.log_view = self.log_view.clone();
        // The live scroll grew with each line kept from bottom since the pause; the frozen log has none.
        view.log_view.scroll = self.log_view.scroll.saturating_sub(self.paused_scroll);
        view.agent_focus = self.agent_focus.clone();
        view.agent_map = self.agent_map;
        view.all_sensors = self.all_sensors;
        view.quit = self.quit;
        view.utc_offset_s = self.utc_offset_s;
        // The frozen lists may be shorter than the live ones: keep every index drawable.
        let models = view.llama.as_ref().map_or(0, |l| l.models.len());
        view.model_selected = self.model_selected.min(models.saturating_sub(1));
        let sessions = view.opencode.as_ref().map_or(0, |o| o.sessions.len());
        view.agent_selected = self.agent_selected.min(sessions.saturating_sub(1));
        Cow::Owned(view)
    }

    /// Starts or ends the freeze: pausing keeps a copy of the data, resuming drops it.
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        self.paused_scroll = 0;
        self.frozen = if self.paused {
            let mut copy = self.clone();
            copy.frozen = None;
            Some(Box::new(copy))
        } else {
            None
        };
    }
}
