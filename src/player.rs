use crate::api::TrackSummary;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerCommand {
    PlayPause,
    Next,
    Previous,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlaybackMode {
    #[default]
    Continue,
    RepeatOne,
}

impl PlaybackMode {
    pub fn toggle(self) -> Self {
        match self {
            PlaybackMode::Continue => PlaybackMode::RepeatOne,
            PlaybackMode::RepeatOne => PlaybackMode::Continue,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PlaybackMode::Continue => "Дальше",
            PlaybackMode::RepeatOne => "Повтор",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerState {
    queue: Vec<String>,
    tracks: Vec<TrackSummary>,
    current_index: Option<usize>,
    is_playing: bool,
}

impl PlayerState {
    pub fn set_queue(&mut self, queue: Vec<String>) {
        self.queue = queue;
        self.tracks.clear();
        self.current_index = if self.queue.is_empty() { None } else { Some(0) };
        self.is_playing = false;
    }

    pub fn set_tracks(&mut self, tracks: Vec<TrackSummary>) {
        self.queue = tracks
            .iter()
            .map(|track| format!("{} — {}", track.artist, track.title))
            .collect();
        self.tracks = tracks;
        self.current_index = if self.queue.is_empty() { None } else { Some(0) };
        self.is_playing = false;
    }

    pub fn current_track(&self) -> Option<&str> {
        self.current_index
            .and_then(|index| self.queue.get(index))
            .map(String::as_str)
    }

    pub fn current_track_summary(&self) -> Option<&TrackSummary> {
        self.current_index.and_then(|index| self.tracks.get(index))
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    pub fn is_current_track_last(&self) -> bool {
        self.current_index
            .is_some_and(|index| index + 1 == self.queue.len())
    }

    pub fn select(&mut self, index: usize) -> bool {
        if index >= self.queue.len() {
            return false;
        }

        self.current_index = Some(index);
        true
    }

    pub fn select_and_play(&mut self, index: usize) -> bool {
        if !self.select(index) {
            return false;
        }

        self.play();
        true
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing
    }

    pub fn play(&mut self) {
        self.is_playing = true;
    }

    pub fn pause(&mut self) {
        self.is_playing = false;
    }

    pub fn apply(&mut self, command: PlayerCommand) {
        match command {
            PlayerCommand::PlayPause => self.is_playing = !self.is_playing,
            PlayerCommand::Next => self.next(),
            PlayerCommand::Previous => self.previous(),
        }
    }

    pub fn next(&mut self) {
        if self.queue.is_empty() {
            self.current_index = None;
            return;
        }

        let next = self
            .current_index
            .map_or(0, |index| (index + 1) % self.queue.len());
        self.current_index = Some(next);
    }

    pub fn previous(&mut self) {
        if self.queue.is_empty() {
            self.current_index = None;
            return;
        }

        let previous = self
            .current_index
            .map_or(0, |index| (index + self.queue.len() - 1) % self.queue.len());
        self.current_index = Some(previous);
    }
}

pub struct Shortcut;

impl Shortcut {
    pub fn from_key(key: &str, ctrl: bool, command: bool) -> Option<PlayerCommand> {
        let _ = (ctrl, command);
        if ctrl || command {
            return None;
        }

        match key {
            "Space" => Some(PlayerCommand::PlayPause),
            "ArrowRight" => Some(PlayerCommand::Next),
            "ArrowLeft" => Some(PlayerCommand::Previous),
            _ => None,
        }
    }
}
