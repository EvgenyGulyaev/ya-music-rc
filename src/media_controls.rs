use std::time::Duration;

use crate::api::TrackSummary;
use crate::player::PlayerCommand;

#[cfg(target_os = "macos")]
use std::sync::mpsc::{self, Receiver};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemMediaEvent {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    SetPosition(Duration),
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemMediaCommand {
    Player(PlayerCommand),
    Play,
    Pause,
    Seek(Duration),
}

pub fn command_for_event(event: SystemMediaEvent) -> Option<SystemMediaCommand> {
    match event {
        SystemMediaEvent::Play => Some(SystemMediaCommand::Play),
        SystemMediaEvent::Pause | SystemMediaEvent::Stop => Some(SystemMediaCommand::Pause),
        SystemMediaEvent::Toggle => Some(SystemMediaCommand::Player(PlayerCommand::PlayPause)),
        SystemMediaEvent::Next => Some(SystemMediaCommand::Player(PlayerCommand::Next)),
        SystemMediaEvent::Previous => Some(SystemMediaCommand::Player(PlayerCommand::Previous)),
        SystemMediaEvent::SetPosition(position) => Some(SystemMediaCommand::Seek(position)),
        SystemMediaEvent::Other => None,
    }
}

#[cfg(target_os = "macos")]
pub struct SystemMediaControls {
    controls: souvlaki::MediaControls,
    rx: Receiver<SystemMediaCommand>,
}

#[cfg(target_os = "macos")]
impl SystemMediaControls {
    pub fn register_if_supported() -> Result<Option<Self>, String> {
        use souvlaki::{MediaControls, PlatformConfig};

        let config = PlatformConfig {
            dbus_name: "ya_player",
            display_name: "Ya Player",
            hwnd: None,
        };
        let mut controls = MediaControls::new(config).map_err(|err| err.to_string())?;
        let (tx, rx) = mpsc::channel();
        controls
            .attach(move |event| {
                if let Some(command) = command_for_event(SystemMediaEvent::from(event)) {
                    let _ = tx.send(command);
                }
            })
            .map_err(|err| err.to_string())?;
        controls
            .set_playback(souvlaki::MediaPlayback::Stopped)
            .map_err(|err| err.to_string())?;

        Ok(Some(Self { controls, rx }))
    }

    pub fn poll_command(&self) -> Option<SystemMediaCommand> {
        self.rx.try_recv().ok()
    }

    pub fn set_track(
        &mut self,
        track: Option<&TrackSummary>,
        duration: Option<Duration>,
        position: Option<Duration>,
        is_playing: bool,
    ) {
        if let Some(track) = track {
            let _ = self.controls.set_metadata(souvlaki::MediaMetadata {
                title: Some(&track.title),
                artist: Some(&track.artist),
                duration,
                ..Default::default()
            });
        }
        self.set_playback(position, is_playing);
    }

    pub fn set_playback(&mut self, position: Option<Duration>, is_playing: bool) {
        use souvlaki::{MediaPlayback, MediaPosition};

        let progress = position.map(MediaPosition);
        let playback = if is_playing {
            MediaPlayback::Playing { progress }
        } else {
            MediaPlayback::Paused { progress }
        };
        let _ = self.controls.set_playback(playback);
    }
}

#[cfg(target_os = "macos")]
impl From<souvlaki::MediaControlEvent> for SystemMediaEvent {
    fn from(event: souvlaki::MediaControlEvent) -> Self {
        match event {
            souvlaki::MediaControlEvent::Play => Self::Play,
            souvlaki::MediaControlEvent::Pause => Self::Pause,
            souvlaki::MediaControlEvent::Toggle => Self::Toggle,
            souvlaki::MediaControlEvent::Next => Self::Next,
            souvlaki::MediaControlEvent::Previous => Self::Previous,
            souvlaki::MediaControlEvent::Stop => Self::Stop,
            souvlaki::MediaControlEvent::SetPosition(position) => Self::SetPosition(position.0),
            _ => Self::Other,
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub struct SystemMediaControls;

#[cfg(not(target_os = "macos"))]
impl SystemMediaControls {
    pub fn register_if_supported() -> Result<Option<Self>, String> {
        Ok(None)
    }

    pub fn poll_command(&self) -> Option<SystemMediaCommand> {
        None
    }

    pub fn set_track(
        &mut self,
        _track: Option<&TrackSummary>,
        _duration: Option<Duration>,
        _position: Option<Duration>,
        _is_playing: bool,
    ) {
    }

    pub fn set_playback(&mut self, _position: Option<Duration>, _is_playing: bool) {}
}
