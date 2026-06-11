use ya_player::api::TrackSummary;
use ya_player::player::{PlaybackMode, PlayerCommand, PlayerState, Shortcut};

#[test]
fn queue_navigation_wraps_around() {
    let mut state = PlayerState::default();
    state.set_queue(vec!["A".to_owned(), "B".to_owned(), "C".to_owned()]);

    assert_eq!(state.current_track(), Some("A"));
    state.next();
    assert_eq!(state.current_track(), Some("B"));
    state.next();
    state.next();
    assert_eq!(state.current_track(), Some("A"));
    state.previous();
    assert_eq!(state.current_track(), Some("C"));
}

#[test]
fn play_pause_toggles_state() {
    let mut state = PlayerState::default();

    assert!(!state.is_playing());
    state.apply(PlayerCommand::PlayPause);
    assert!(state.is_playing());
    state.apply(PlayerCommand::PlayPause);
    assert!(!state.is_playing());
}

#[test]
fn play_and_pause_can_set_state_explicitly() {
    let mut state = PlayerState::default();

    state.play();
    state.play();
    assert!(state.is_playing());

    state.pause();
    state.pause();
    assert!(!state.is_playing());
}

#[test]
fn playback_mode_toggles_between_continue_and_repeat_one() {
    assert_eq!(PlaybackMode::Continue.toggle(), PlaybackMode::RepeatOne);
    assert_eq!(PlaybackMode::RepeatOne.toggle(), PlaybackMode::Continue);
    assert_eq!(PlaybackMode::Continue.label(), "След");
    assert_eq!(PlaybackMode::RepeatOne.label(), "Повтор");
}

#[test]
fn shortcuts_map_to_track_commands() {
    assert_eq!(
        Shortcut::from_key("Space", false, false),
        Some(PlayerCommand::PlayPause)
    );
    assert_eq!(
        Shortcut::from_key("ArrowRight", false, false),
        Some(PlayerCommand::Next)
    );
    assert_eq!(
        Shortcut::from_key("ArrowLeft", false, false),
        Some(PlayerCommand::Previous)
    );
    assert_eq!(Shortcut::from_key("ArrowRight", true, false), None);
    assert_eq!(Shortcut::from_key("ArrowLeft", false, true), None);
    assert_eq!(Shortcut::from_key("KeyA", false, false), None);
}

#[test]
fn queue_can_store_track_metadata_for_playback() {
    let mut state = PlayerState::default();
    state.set_tracks(vec![TrackSummary {
        id: "100".to_owned(),
        album_id: Some("200".to_owned()),
        title: "Song".to_owned(),
        artist: "Artist".to_owned(),
    }]);

    let track = state.current_track_summary().expect("current track");

    assert_eq!(track.id, "100");
    assert_eq!(track.album_id.as_deref(), Some("200"));
    assert_eq!(state.current_track(), Some("Artist — Song"));
}

#[test]
fn queue_selection_can_move_to_a_specific_track() {
    let mut state = PlayerState::default();
    state.set_tracks(vec![
        TrackSummary {
            id: "100".to_owned(),
            album_id: Some("200".to_owned()),
            title: "First".to_owned(),
            artist: "Artist".to_owned(),
        },
        TrackSummary {
            id: "101".to_owned(),
            album_id: Some("201".to_owned()),
            title: "Second".to_owned(),
            artist: "Artist".to_owned(),
        },
    ]);

    assert_eq!(state.current_index(), Some(0));
    assert!(state.select(1));
    assert_eq!(state.current_index(), Some(1));
    assert_eq!(state.current_track(), Some("Artist — Second"));

    assert!(!state.select(9));
    assert_eq!(state.current_index(), Some(1));
}

#[test]
fn queue_selection_can_start_playback_for_clicked_track() {
    let mut state = PlayerState::default();
    state.set_tracks(vec![
        TrackSummary {
            id: "100".to_owned(),
            album_id: Some("200".to_owned()),
            title: "First".to_owned(),
            artist: "Artist".to_owned(),
        },
        TrackSummary {
            id: "101".to_owned(),
            album_id: Some("201".to_owned()),
            title: "Second".to_owned(),
            artist: "Artist".to_owned(),
        },
    ]);

    assert!(!state.is_playing());
    assert!(state.select_and_play(1));

    assert_eq!(state.current_index(), Some(1));
    assert_eq!(state.current_track(), Some("Artist — Second"));
    assert!(state.is_playing());
}

#[test]
fn queue_knows_when_current_track_is_last() {
    let mut state = PlayerState::default();
    state.set_queue(vec!["A".to_owned(), "B".to_owned()]);

    assert!(!state.is_current_track_last());
    state.next();
    assert!(state.is_current_track_last());

    state.set_queue(Vec::new());
    assert!(!state.is_current_track_last());
}
