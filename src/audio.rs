use std::fs::File;

pub struct AudioPlayer {
    _sink: rodio::MixerDeviceSink,
    player: rodio::Player,
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
        })
    }

    pub fn play_file(&mut self, file: File) -> Result<(), String> {
        self.player.stop();
        let source =
            rodio::Decoder::try_from(file).map_err(|err| format!("audio decode error: {err}"))?;
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

    pub fn is_empty(&self) -> bool {
        self.player.empty()
    }
}
