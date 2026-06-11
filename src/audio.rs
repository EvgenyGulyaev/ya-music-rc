use std::fs::File;
use std::time::Duration;

use rodio::Source;

pub struct AudioPlayer {
    _sink: rodio::MixerDeviceSink,
    player: rodio::Player,
    duration: Option<Duration>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self, String> {
        let mut sink = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|err| format!("audio device error: {err}"))?;
        sink.log_on_drop(false);
        let player = rodio::Player::connect_new(sink.mixer());

        Ok(Self {
            _sink: sink,
            player,
            duration: None,
        })
    }

    pub fn play_file(&mut self, file: File) -> Result<(), String> {
        self.player.stop();
        let source =
            rodio::Decoder::try_from(file).map_err(|err| format!("audio decode error: {err}"))?;
        self.duration = source.total_duration();
        self.player.append(source);
        self.player.play();
        Ok(())
    }

    pub fn pause(&self) {
        self.player.pause();
    }

    pub fn resume(&self) {
        self.player.play();
    }

    pub fn stop(&self) {
        self.player.stop();
    }

    pub fn set_volume(&self, multiplier: f32) {
        self.player.set_volume(multiplier);
    }

    pub fn position(&self) -> Duration {
        match self.duration {
            Some(duration) => self.player.get_pos().min(duration),
            None => self.player.get_pos(),
        }
    }

    pub fn duration(&self) -> Option<Duration> {
        self.duration
    }

    pub fn seek(&self, position: Duration) -> Result<(), String> {
        let position = match self.duration {
            Some(duration) => position.min(duration),
            None => position,
        };
        self.player
            .try_seek(position)
            .map_err(|err| format!("seek error: {err}"))
    }

    pub fn is_empty(&self) -> bool {
        self.player.empty()
    }
}
