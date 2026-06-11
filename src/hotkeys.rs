use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::player::PlayerCommand;

pub struct MediaHotkeys {
    _manager: GlobalHotKeyManager,
    play_pause_ids: Vec<u32>,
    next_ids: Vec<u32>,
    previous_ids: Vec<u32>,
    warnings: Vec<String>,
}

impl MediaHotkeys {
    pub fn register() -> Result<Self, String> {
        let manager = GlobalHotKeyManager::new().map_err(|err| err.to_string())?;
        let mut hotkeys = Self {
            _manager: manager,
            play_pause_ids: Vec::new(),
            next_ids: Vec::new(),
            previous_ids: Vec::new(),
            warnings: Vec::new(),
        };

        hotkeys.register_one(None, Code::MediaPlayPause, PlayerCommand::PlayPause);
        hotkeys.register_one(None, Code::MediaTrackNext, PlayerCommand::Next);
        hotkeys.register_one(None, Code::MediaFastForward, PlayerCommand::Next);
        hotkeys.register_one(None, Code::MediaTrackPrevious, PlayerCommand::Previous);
        hotkeys.register_one(None, Code::MediaRewind, PlayerCommand::Previous);

        if hotkeys.play_pause_ids.is_empty()
            && hotkeys.next_ids.is_empty()
            && hotkeys.previous_ids.is_empty()
        {
            Err(hotkeys.warnings.join("; "))
        } else {
            Ok(hotkeys)
        }
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn poll_command(&self) -> Option<PlayerCommand> {
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state != HotKeyState::Pressed {
                continue;
            }
            if let Some(command) = self.command_for_id(event.id) {
                return Some(command);
            }
        }

        None
    }

    pub fn command_for_id(&self, id: u32) -> Option<PlayerCommand> {
        command_for_id(id, &self.play_pause_ids, &self.next_ids, &self.previous_ids)
    }

    fn register_one(&mut self, mods: Option<Modifiers>, code: Code, command: PlayerCommand) {
        let hotkey = HotKey::new(mods, code);
        match self._manager.register(hotkey) {
            Ok(()) => self.push_id(command, hotkey.id()),
            Err(err) => self.warnings.push(format!("{hotkey}: {err}")),
        }
    }

    fn push_id(&mut self, command: PlayerCommand, id: u32) {
        match command {
            PlayerCommand::PlayPause => self.play_pause_ids.push(id),
            PlayerCommand::Next => self.next_ids.push(id),
            PlayerCommand::Previous => self.previous_ids.push(id),
        }
    }
}

pub fn command_for_id(
    id: u32,
    play_pause_ids: &[u32],
    next_ids: &[u32],
    previous_ids: &[u32],
) -> Option<PlayerCommand> {
    if play_pause_ids.contains(&id) {
        Some(PlayerCommand::PlayPause)
    } else if next_ids.contains(&id) {
        Some(PlayerCommand::Next)
    } else if previous_ids.contains(&id) {
        Some(PlayerCommand::Previous)
    } else {
        None
    }
}
