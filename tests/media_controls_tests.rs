use std::time::Duration;

use ya_player::media_controls::{SystemMediaCommand, SystemMediaEvent, command_for_event};
use ya_player::player::PlayerCommand;

#[test]
fn system_media_events_map_to_player_commands() {
    assert_eq!(
        command_for_event(SystemMediaEvent::Toggle),
        Some(SystemMediaCommand::Player(PlayerCommand::PlayPause))
    );
    assert_eq!(
        command_for_event(SystemMediaEvent::Next),
        Some(SystemMediaCommand::Player(PlayerCommand::Next))
    );
    assert_eq!(
        command_for_event(SystemMediaEvent::Previous),
        Some(SystemMediaCommand::Player(PlayerCommand::Previous))
    );
}

#[test]
fn absolute_system_media_events_keep_their_intent() {
    assert_eq!(
        command_for_event(SystemMediaEvent::Play),
        Some(SystemMediaCommand::Play)
    );
    assert_eq!(
        command_for_event(SystemMediaEvent::Pause),
        Some(SystemMediaCommand::Pause)
    );
    assert_eq!(
        command_for_event(SystemMediaEvent::Stop),
        Some(SystemMediaCommand::Pause)
    );
}

#[test]
fn system_media_seek_event_carries_position() {
    assert_eq!(
        command_for_event(SystemMediaEvent::SetPosition(Duration::from_secs(42))),
        Some(SystemMediaCommand::Seek(Duration::from_secs(42)))
    );
}
