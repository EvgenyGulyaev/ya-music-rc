use ya_player::hotkeys::command_for_id;
use ya_player::player::PlayerCommand;

#[test]
fn hotkey_ids_map_to_player_commands() {
    assert_eq!(
        command_for_id(10, &[10, 11], &[20], &[30]),
        Some(PlayerCommand::PlayPause)
    );
    assert_eq!(
        command_for_id(20, &[10], &[20], &[30]),
        Some(PlayerCommand::Next)
    );
    assert_eq!(
        command_for_id(30, &[10], &[20], &[30]),
        Some(PlayerCommand::Previous)
    );
    assert_eq!(command_for_id(99, &[10], &[20], &[30]), None);
}

#[test]
fn hotkey_ids_allow_media_aliases_for_next_and_previous() {
    assert_eq!(
        command_for_id(21, &[10], &[20, 21], &[30, 31]),
        Some(PlayerCommand::Next)
    );
    assert_eq!(
        command_for_id(31, &[10], &[20, 21], &[30, 31]),
        Some(PlayerCommand::Previous)
    );
}
