use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::egui;

use crate::api::{AccountStatus, ReqwestHttpClient, TrackSummary, WaveStation, YandexMusicClient};
use crate::audio::AudioPlayer;
use crate::auth::{authorize_url, extract_oauth_token, extract_oauth_token_from_redirect};
use crate::config::AppConfig;
use crate::hotkeys::MediaHotkeys;
use crate::media_controls::{SystemMediaCommand, SystemMediaControls};
use crate::player::{PlaybackMode, PlayerCommand, PlayerState, Shortcut};

const PLAYER_BAR_HEIGHT: f32 = 72.0;
const MAX_VOLUME_PERCENT: u8 = 150;
const FAVORITE_ROW_FONT_SIZE: f32 = 20.0;

pub struct YaPlayerApp {
    config: AppConfig,
    token_input: String,
    status: String,
    account: Option<AccountStatus>,
    favorites: Vec<TrackSummary>,
    search_input: String,
    search_results: Vec<TrackSummary>,
    wave_stations: Vec<WaveStation>,
    active_wave_station: Option<WaveStation>,
    wave_tracks: Vec<TrackSummary>,
    player: PlayerState,
    queue_source: QueueSource,
    playback_mode: PlaybackMode,
    audio: Option<AudioPlayer>,
    volume_percent: u8,
    loaded_audio_track_id: Option<String>,
    output_device_id: Option<String>,
    media_controls: Option<SystemMediaControls>,
    hotkeys: Option<MediaHotkeys>,
    tx: Sender<UiMessage>,
    rx: Receiver<UiMessage>,
    busy: bool,
    audio_busy: bool,
}

impl Default for YaPlayerApp {
    fn default() -> Self {
        let config = AppConfig::load().unwrap_or_default();
        let token_input = config.token.clone().unwrap_or_default();
        let volume_percent = config.volume_percent.min(MAX_VOLUME_PERCENT);
        let (tx, rx) = mpsc::channel();
        let (media_controls, media_controls_error) =
            match SystemMediaControls::register_if_supported() {
                Ok(media_controls) => (media_controls, None),
                Err(err) => (None, Some(err)),
            };
        let (hotkeys, mut hotkey_status) = if media_controls.is_some() {
            (None, "Введите OAuth token и проверьте вход".to_owned())
        } else {
            match MediaHotkeys::register() {
                Ok(hotkeys) => {
                    let status = if hotkeys.warnings().is_empty() {
                        "Введите OAuth token и проверьте вход".to_owned()
                    } else {
                        format!(
                            "Часть глобальных клавиш не зарегистрировалась: {}",
                            hotkeys.warnings().join("; ")
                        )
                    };
                    (Some(hotkeys), status)
                }
                Err(err) => (
                    None,
                    format!(
                        "Глобальные media keys недоступны: {err}. В окне работают Space и стрелки."
                    ),
                ),
            }
        };
        if let Some(err) = media_controls_error {
            hotkey_status = format!("Системные media keys недоступны: {err}. {hotkey_status}");
        }

        let mut app = Self {
            config,
            token_input,
            status: hotkey_status,
            account: None,
            favorites: Vec::new(),
            search_input: String::new(),
            search_results: Vec::new(),
            wave_stations: Vec::new(),
            active_wave_station: None,
            wave_tracks: Vec::new(),
            player: PlayerState::default(),
            queue_source: QueueSource::Favorites,
            playback_mode: PlaybackMode::Continue,
            audio: None,
            volume_percent,
            loaded_audio_track_id: None,
            output_device_id: default_output_device_id(),
            media_controls,
            hotkeys,
            tx,
            rx,
            busy: false,
            audio_busy: false,
        };

        if !app.token_input.trim().is_empty() {
            app.start_bootstrap("Загружаю аккаунт...");
        }
        app.watch_output_device();

        app
    }
}

impl eframe::App for YaPlayerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        self.handle_system_media_controls();
        self.handle_global_hotkeys();
        self.handle_shortcuts(ctx);
        self.receive_messages();
        self.handle_playback_completion();

        egui::TopBottomPanel::bottom("player_bar")
            .resizable(false)
            .exact_height(player_bar_height())
            .show(ctx, |ui| {
                self.player_bar(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let current_track = self.player.current_track_summary().cloned();
            let favorite_rows = self.favorites.clone();
            let search_rows = self.search_results.clone();
            let wave_stations = self.wave_stations.clone();
            let mut favorite_clicked = None;
            let mut search_clicked = None;
            let mut load_favorites_clicked = false;
            let mut play_wave_clicked = false;
            let mut shuffle_wave_clicked = false;
            let mut wave_station_clicked = None;

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Воспроизведение").strong());
                ui.separator();
                ui.label(egui::RichText::new("Поиск").weak());
                let button_width = 88.0;
                let input_width =
                    (ui.available_width() - button_width - ui.spacing().item_spacing.x).max(280.0);
                let search_response = ui.add_sized(
                    [input_width, 34.0],
                    egui::TextEdit::singleline(&mut self.search_input)
                        .hint_text("Трек, артист или альбом"),
                );
                let enter_pressed = search_response.lost_focus()
                    && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Найти"))
                    .clicked()
                    || enter_pressed
                {
                    self.search_tracks();
                }

                if !search_rows.is_empty() {
                    let popup_pos = search_response.rect.left_bottom() + egui::vec2(0.0, 6.0);
                    let popup_width =
                        search_response.rect.width() + button_width + ui.spacing().item_spacing.x;
                    egui::Area::new(egui::Id::new("search_results_popup"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(popup_pos)
                        .show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style())
                                .inner_margin(egui::Margin::symmetric(10, 8))
                                .show(ui, |ui| {
                                    ui.set_min_width(popup_width);
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Результаты поиска")
                                                .size(15.0)
                                                .weak(),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui.small_button("Очистить").clicked() {
                                                    self.search_results.clear();
                                                }
                                            },
                                        );
                                    });
                                    ui.add_space(4.0);
                                    egui::ScrollArea::vertical()
                                        .id_salt("search_results_popup_scroll")
                                        .max_height(260.0)
                                        .show(ui, |ui| {
                                            for (index, track) in search_rows.iter().enumerate() {
                                                let selected =
                                                    is_same_track(current_track.as_ref(), track);
                                                let row =
                                                    format!("{} — {}", track.artist, track.title);
                                                let response = ui.add_sized(
                                                    [popup_width - 8.0, 32.0],
                                                    egui::Button::selectable(
                                                        selected,
                                                        egui::RichText::new(row)
                                                            .size(FAVORITE_ROW_FONT_SIZE),
                                                    ),
                                                );
                                                if response.clicked() {
                                                    search_clicked = Some(index);
                                                }
                                            }
                                        });
                                });
                        });
                }
            });

            if self.account.is_none() && !(self.busy && !self.token_input.trim().is_empty()) {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Token");
                    let token = egui::TextEdit::singleline(&mut self.token_input)
                        .password(true)
                        .hint_text("OAuth token или redirect URL");
                    ui.add_sized([360.0, 24.0], token);
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("Войти через Яндекс"))
                        .clicked()
                    {
                        self.open_yandex_login();
                    }
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("Проверить вход"))
                        .clicked()
                    {
                        self.check_login();
                    }
                });
                if !self.status.is_empty() {
                    ui.label(egui::RichText::new(&self.status).small());
                }
            }

            ui.separator();

            ui.columns(2, |columns| {
                if columns[0]
                    .add_enabled(
                        self.account.is_some() && !self.busy,
                        egui::Button::new(egui::RichText::new("Любимое").size(24.0)).frame(false),
                    )
                    .on_hover_text("Загрузить любимые треки")
                    .clicked()
                {
                    load_favorites_clicked = true;
                }
                let favorites_height = favorites_list_height(columns[0].available_height());
                egui::ScrollArea::vertical()
                    .id_salt("favorites")
                    .max_height(favorites_height)
                    .show(&mut columns[0], |ui| {
                        if favorite_rows.is_empty() {
                            ui.label("Пока пусто. Нажмите заголовок «Любимое» после входа.");
                        }
                        for (index, track) in favorite_rows.iter().enumerate() {
                            let selected = is_same_track(current_track.as_ref(), track);
                            let prefix = if selected { "▶ " } else { "" };
                            let row = format!("{prefix}{} — {}", track.artist, track.title);
                            if ui
                                .selectable_label(
                                    selected,
                                    egui::RichText::new(row).size(FAVORITE_ROW_FONT_SIZE),
                                )
                                .clicked()
                            {
                                favorite_clicked = Some(index);
                            }
                        }
                    });

                if columns[1]
                    .add_enabled(
                        !self.busy,
                        egui::Button::new(egui::RichText::new("Волна").size(24.0)).frame(false),
                    )
                    .on_hover_text("Запустить волну")
                    .clicked()
                {
                    play_wave_clicked = true;
                }
                columns[1].add_space(10.0);
                let wave_label =
                    if self.queue_source == QueueSource::Wave && self.player.is_playing() {
                        "Волна играет"
                    } else if self.wave_tracks.is_empty() {
                        "Выберите подборку или запустите свою волну."
                    } else {
                        "Волна готова. Нажмите play."
                    };
                columns[1].label(wave_label);
                columns[1].add_space(8.0);
                columns[1].horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            !self.busy,
                            egui::Button::new("▶ Моя волна").min_size([140.0, 36.0].into()),
                        )
                        .on_hover_text("Запустить мою волну")
                        .clicked()
                    {
                        play_wave_clicked = true;
                    }
                    if ui
                        .add_enabled(
                            !self.busy,
                            egui::Button::new("Перемешать").min_size([120.0, 36.0].into()),
                        )
                        .on_hover_text("Обновить текущую волну")
                        .clicked()
                    {
                        shuffle_wave_clicked = true;
                    }
                });
                columns[1].add_space(10.0);
                columns[1].label(egui::RichText::new("Подборки").size(18.0));
                columns[1].horizontal_wrapped(|ui| {
                    for station in wave_stations.iter().take(10) {
                        let selected = self
                            .active_wave_station
                            .as_ref()
                            .is_some_and(|active| same_station(active, station));
                        if ui
                            .selectable_label(selected, station.name.as_str())
                            .on_hover_text("Запустить волну в этом стиле")
                            .clicked()
                        {
                            wave_station_clicked = Some(station.clone());
                        }
                    }
                });
            });

            if load_favorites_clicked {
                self.load_favorites();
            }
            if play_wave_clicked {
                self.play_my_wave();
            }
            if shuffle_wave_clicked {
                self.shuffle_wave();
            }
            if let Some(station) = wave_station_clicked {
                self.load_wave_station(station, true);
            }
            if let Some(index) = favorite_clicked {
                self.select_favorite_track(index);
            }
            if let Some(index) = search_clicked {
                self.select_search_track(index);
            }
        });
    }
}

impl YaPlayerApp {
    fn handle_system_media_controls(&mut self) {
        let Some(media_controls) = &self.media_controls else {
            return;
        };
        let mut commands = Vec::new();
        while let Some(command) = media_controls.poll_command() {
            commands.push(command);
        }
        for command in commands {
            self.handle_system_media_command(command);
        }
    }

    fn handle_global_hotkeys(&mut self) {
        let Some(hotkeys) = &self.hotkeys else {
            return;
        };
        let mut commands = Vec::new();
        while let Some(command) = hotkeys.poll_command() {
            commands.push(command);
        }
        for command in commands {
            self.handle_player_command(command);
        }
    }

    fn handle_playback_completion(&mut self) {
        if self.audio_busy || !self.player.is_playing() {
            return;
        }
        let Some(audio) = &self.audio else {
            return;
        };
        if !audio.is_empty() || !self.can_resume_current_track() {
            return;
        }

        match self.playback_mode {
            PlaybackMode::Continue => {
                if self.queue_source == QueueSource::Wave && self.player.is_current_track_last() {
                    self.load_wave(true);
                    return;
                }
                self.player.next();
            }
            PlaybackMode::RepeatOne => {}
        }
        self.play_current_track();
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let mut commands = Vec::new();
        ctx.input(|input| {
            let candidates = [
                (egui::Key::Space, "Space"),
                (egui::Key::ArrowRight, "ArrowRight"),
                (egui::Key::ArrowLeft, "ArrowLeft"),
            ];
            for (key, name) in candidates {
                if input.key_pressed(key) {
                    if let Some(command) =
                        Shortcut::from_key(name, input.modifiers.ctrl, input.modifiers.command)
                    {
                        commands.push(command);
                    }
                }
            }
        });

        for command in commands {
            self.handle_player_command(command);
        }
    }

    fn receive_messages(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            match message {
                UiMessage::Audio(result) => {
                    self.audio_busy = false;
                    match result {
                        Ok((file, track_id, _title)) => match self.audio_player() {
                            Ok(audio) => match audio.play_file(file) {
                                Ok(()) => {
                                    self.player.play();
                                    self.loaded_audio_track_id = Some(track_id);
                                    self.status = "Воспроизведение".to_owned();
                                    self.update_system_media_state();
                                }
                                Err(err) => {
                                    self.player.pause();
                                    self.status = err;
                                    self.update_system_media_state();
                                }
                            },
                            Err(err) => {
                                self.player.pause();
                                self.status = err;
                                self.update_system_media_state();
                            }
                        },
                        Err(err) => {
                            self.player.pause();
                            self.status = err;
                            self.update_system_media_state();
                        }
                    }
                }
                UiMessage::Bootstrap(result) => {
                    match result {
                        Ok(data) => {
                            self.config.token = Some(self.token_input.trim().to_owned());
                            let save_error = self.config.save().err().map(|err| err.to_string());

                            let mut status_parts =
                                vec![format!("Аккаунт: {}", data.account.display_name)];

                            match data.favorites {
                                Ok(tracks) => {
                                    status_parts.push(format!("любимых: {}", tracks.len()));
                                    if !tracks.is_empty() {
                                        self.player.set_tracks(tracks.clone());
                                        self.queue_source = QueueSource::Favorites;
                                        self.loaded_audio_track_id = None;
                                    }
                                    self.favorites = tracks;
                                }
                                Err(err) => {
                                    status_parts.push(format!("любимое не загрузилось: {err}"));
                                }
                            }

                            match data.wave {
                                Ok(wave) => {
                                    status_parts.push(format!(
                                        "волна: {} треков, {} станций",
                                        wave.tracks.len(),
                                        wave.stations.len()
                                    ));
                                    if self.favorites.is_empty() && !wave.tracks.is_empty() {
                                        self.player.set_tracks(wave.tracks.clone());
                                        self.queue_source = QueueSource::Wave;
                                        self.loaded_audio_track_id = None;
                                    }
                                    self.active_wave_station = wave.station.clone();
                                    self.wave_stations = wave.stations;
                                    self.wave_tracks = wave.tracks;
                                }
                                Err(err) => {
                                    status_parts.push(format!("волна не загрузилась: {err}"));
                                }
                            }

                            if let Some(err) = save_error {
                                status_parts.push(format!("config не сохранён: {err}"));
                            }
                            self.account = Some(data.account);
                            self.status = status_parts.join("; ");
                            self.update_system_media_state();
                        }
                        Err(err) => self.status = err,
                    }
                    self.busy = false;
                }
                UiMessage::Favorites(result) => {
                    match result {
                        Ok(tracks) => {
                            self.status = format!("Загружено любимых треков: {}", tracks.len());
                            self.player.set_tracks(tracks.clone());
                            self.queue_source = QueueSource::Favorites;
                            self.favorites = tracks;
                            self.loaded_audio_track_id = None;
                            self.update_system_media_state();
                        }
                        Err(err) => self.status = err,
                    }
                    self.busy = false;
                }
                UiMessage::Search(result) => {
                    match result {
                        Ok(tracks) => {
                            self.status = format!("Найдено треков: {}", tracks.len());
                            self.search_results = tracks;
                        }
                        Err(err) => self.status = err,
                    }
                    self.busy = false;
                }
                UiMessage::Wave(result, autoplay) => {
                    match result {
                        Ok(data) => {
                            self.status = format!(
                                "Волна: {} треков, станций: {} ({})",
                                data.tracks.len(),
                                data.stations.len(),
                                data.diagnostics
                            );
                            if !data.tracks.is_empty() {
                                self.player.set_tracks(data.tracks.clone());
                                self.queue_source = QueueSource::Wave;
                                self.loaded_audio_track_id = None;
                                if autoplay {
                                    self.player.play();
                                    self.play_current_track();
                                }
                            }
                            self.active_wave_station = data.station.clone();
                            self.wave_stations = data.stations;
                            self.wave_tracks = data.tracks;
                            self.update_system_media_state();
                        }
                        Err(err) => self.status = err,
                    }
                    self.busy = false;
                }
                UiMessage::TokenCaptured(result) => match result {
                    Ok(token) => {
                        self.token_input = token;
                        self.status = "Token получен из браузера, проверяю вход...".to_owned();
                        self.start_bootstrap("Token получен из браузера, загружаю аккаунт...");
                    }
                    Err(err) => self.status = err,
                },
                UiMessage::OutputDeviceChanged(device_id) => {
                    let previous = self.output_device_id.clone();
                    if should_pause_for_output_change(
                        previous.as_deref(),
                        device_id.as_deref(),
                        self.player.is_playing(),
                    ) {
                        self.player.pause();
                        if let Some(audio) = &self.audio {
                            audio.pause();
                        }
                        self.status = "Аудиовывод изменился. Поставил паузу.".to_owned();
                        self.update_system_media_state();
                    }
                    self.output_device_id = device_id;
                }
            }
        }
    }

    fn check_login(&mut self) {
        self.start_bootstrap("Проверяю вход и загружаю музыку...");
    }

    fn start_bootstrap(&mut self, status: &str) {
        let Some(token) = self.valid_token() else {
            return;
        };
        self.spawn_request(status, move || {
            let client = YandexMusicClient::new(token, ReqwestHttpClient::default());
            let account = match client.account_status() {
                Ok(account) => account,
                Err(err) => return UiMessage::Bootstrap(Err(err.to_string())),
            };
            let favorites = client
                .liked_tracks(account.uid)
                .map_err(|err| err.to_string());
            let wave = load_wave_data(&client, None).map_err(|err| err.to_string());

            UiMessage::Bootstrap(Ok(BootstrapData {
                account,
                favorites,
                wave,
            }))
        });
    }

    fn load_favorites(&mut self) {
        let Some(token) = self.valid_token() else {
            return;
        };
        let Some(uid) = self.account.as_ref().map(|account| account.uid) else {
            self.status = "Сначала проверьте вход".to_owned();
            return;
        };
        self.spawn_request("Загружаю любимое...", move || {
            let client = YandexMusicClient::new(token, ReqwestHttpClient::default());
            UiMessage::Favorites(client.liked_tracks(uid).map_err(|err| err.to_string()))
        });
    }

    fn load_wave(&mut self, autoplay: bool) {
        let station = self.active_wave_station.clone();
        self.load_wave_station_optional(station, autoplay);
    }

    fn load_wave_station(&mut self, station: WaveStation, autoplay: bool) {
        self.load_wave_station_optional(Some(station), autoplay);
    }

    fn load_wave_station_optional(&mut self, station: Option<WaveStation>, autoplay: bool) {
        let Some(token) = self.valid_token() else {
            return;
        };
        self.spawn_request("Загружаю волну...", move || {
            let client = YandexMusicClient::new(token, ReqwestHttpClient::default());
            UiMessage::Wave(
                load_wave_data(&client, station).map_err(|err| err.to_string()),
                autoplay,
            )
        });
    }

    fn search_tracks(&mut self) {
        let query = self.search_input.trim().to_owned();
        if query.is_empty() {
            self.search_results.clear();
            return;
        }
        let Some(token) = self.valid_token() else {
            return;
        };
        self.spawn_request("Ищу треки...", move || {
            let client = YandexMusicClient::new(token, ReqwestHttpClient::default());
            UiMessage::Search(client.search_tracks(&query).map_err(|err| err.to_string()))
        });
    }

    fn select_favorite_track(&mut self, index: usize) {
        self.select_track_from_list(self.favorites.clone(), index);
    }

    fn select_search_track(&mut self, index: usize) {
        self.select_track_from_source(self.search_results.clone(), index, QueueSource::Search);
    }

    fn play_my_wave(&mut self) {
        let station = self.my_wave_station();
        let my_wave_already_loaded = station.as_ref().is_some_and(|station| {
            self.active_wave_station
                .as_ref()
                .is_some_and(|active| same_station(active, station))
                && !self.wave_tracks.is_empty()
        });

        if !my_wave_already_loaded {
            self.load_wave_station_optional(station, true);
            return;
        }

        self.select_track_from_source(self.wave_tracks.clone(), 0, QueueSource::Wave);
    }

    fn shuffle_wave(&mut self) {
        self.load_wave(true);
    }

    fn my_wave_station(&self) -> Option<WaveStation> {
        self.wave_stations
            .iter()
            .find(|station| station.tag == "onyourwave")
            .cloned()
            .or_else(|| self.wave_stations.first().cloned())
    }

    fn select_track_from_list(&mut self, tracks: Vec<TrackSummary>, index: usize) {
        self.select_track_from_source(tracks, index, QueueSource::Favorites);
    }

    fn select_track_from_source(
        &mut self,
        tracks: Vec<TrackSummary>,
        index: usize,
        source: QueueSource,
    ) {
        if let Some(audio) = &self.audio {
            audio.stop();
        }

        self.player.set_tracks(tracks);
        self.queue_source = source;
        if !self.player.select_and_play(index) {
            self.status = "Не смог выбрать трек".to_owned();
            return;
        }

        self.loaded_audio_track_id = None;
        if let Some(track) = self.player.current_track() {
            self.status = format!("Выбрано: {track}");
        }
        self.play_current_track();
    }

    fn open_yandex_login(&mut self) {
        match open::that(authorize_url()) {
            Ok(()) => {
                self.status = "Открыл браузер. Жду access_token из адресной строки...".to_owned();
                self.watch_browser_for_token();
            }
            Err(err) => {
                self.status = format!("Не смог открыть браузер: {err}");
            }
        }
    }

    fn valid_token(&mut self) -> Option<String> {
        let Some(token) = extract_oauth_token(&self.token_input) else {
            self.status = "Token пустой".to_owned();
            return None;
        };

        self.token_input = token.clone();
        Some(token)
    }

    fn watch_browser_for_token(&self) {
        let tx = self.tx.clone();
        thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(120);
            while Instant::now() < deadline {
                match current_browser_urls() {
                    Ok(urls) => {
                        if let Some(token) = urls
                            .iter()
                            .find_map(|url| extract_oauth_token_from_redirect(url))
                        {
                            let _ = tx.send(UiMessage::TokenCaptured(Ok(token)));
                            return;
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(UiMessage::TokenCaptured(Err(format!(
                            "Не смог прочитать URL браузера: {err}. Можно вставить redirect URL вручную."
                        ))));
                        return;
                    }
                }
                thread::sleep(Duration::from_millis(700));
            }

            let _ = tx.send(UiMessage::TokenCaptured(Err(
                "Не дождался access_token из браузера. Вставьте redirect URL вручную.".to_owned(),
            )));
        });
    }

    fn watch_output_device(&self) {
        let tx = self.tx.clone();
        let mut previous = self.output_device_id.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(1));
                let current = default_output_device_id();
                if current != previous {
                    previous = current.clone();
                    let _ = tx.send(UiMessage::OutputDeviceChanged(current));
                }
            }
        });
    }

    fn spawn_request<F>(&mut self, status: &str, request: F)
    where
        F: FnOnce() -> UiMessage + Send + 'static,
    {
        self.busy = true;
        self.status = status.to_owned();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let _ = tx.send(request());
        });
    }

    fn handle_player_command(&mut self, command: PlayerCommand) {
        if self.audio_busy {
            self.status = "Audio: готовлю текущий трек...".to_owned();
            return;
        }

        match command {
            PlayerCommand::PlayPause => {
                let was_playing = self.player.is_playing();
                self.player.apply(command);
                if was_playing {
                    if let Some(audio) = &self.audio {
                        audio.pause();
                    }
                    self.update_system_media_state();
                } else if self.can_resume_current_track() {
                    let audio = self.audio.as_ref().expect("checked audio");
                    audio.resume();
                    self.update_system_media_state();
                } else {
                    self.play_current_track();
                }
            }
            PlayerCommand::Next => {
                let should_continue = self.player.is_playing();
                if self.queue_source == QueueSource::Wave && self.player.is_current_track_last() {
                    self.load_wave(should_continue);
                    return;
                }

                self.player.apply(command);
                self.update_system_media_state();
                if should_continue {
                    self.play_current_track();
                }
            }
            PlayerCommand::Previous => {
                let should_continue = self.player.is_playing();
                self.player.apply(command);
                self.update_system_media_state();
                if should_continue {
                    self.play_current_track();
                }
            }
        }
    }

    fn handle_system_media_command(&mut self, command: SystemMediaCommand) {
        match command {
            SystemMediaCommand::Player(command) => self.handle_player_command(command),
            SystemMediaCommand::Play => {
                if !self.player.is_playing() {
                    self.handle_player_command(PlayerCommand::PlayPause);
                }
            }
            SystemMediaCommand::Pause => {
                if self.player.is_playing() {
                    self.handle_player_command(PlayerCommand::PlayPause);
                }
            }
            SystemMediaCommand::Seek(position) => {
                if let Some(audio) = &self.audio {
                    if let Err(err) = audio.seek(position) {
                        self.status = err;
                    } else {
                        self.update_system_media_state();
                    }
                }
            }
        }
    }

    fn play_current_track(&mut self) {
        if self.audio_busy {
            return;
        }
        let Some(token) = self.valid_token() else {
            self.player.pause();
            return;
        };
        let Some(track) = self.player.current_track_summary().cloned() else {
            self.status = "Сначала загрузите любимое и выберите трек".to_owned();
            self.player.pause();
            return;
        };

        self.audio_busy = true;
        self.status = format!("Готовлю: {} — {}", track.artist, track.title);
        let tx = self.tx.clone();
        thread::spawn(move || {
            let title = format!("{} — {}", track.artist, track.title);
            let track_id = track.id.clone();
            let result = fetch_track_audio(token, &track)
                .map(|file| (file, track_id, title))
                .map_err(|err| err.to_string());
            let _ = tx.send(UiMessage::Audio(result));
        });
    }

    fn can_resume_current_track(&self) -> bool {
        let Some(audio_track_id) = self.loaded_audio_track_id.as_deref() else {
            return false;
        };
        let Some(current_track) = self.player.current_track_summary() else {
            return false;
        };

        self.audio.is_some() && current_track.id == audio_track_id
    }

    fn audio_player(&mut self) -> Result<&mut AudioPlayer, String> {
        if self.audio.is_none() {
            let audio = AudioPlayer::new()?;
            audio.set_volume(volume_multiplier(self.volume_percent));
            self.audio = Some(audio);
        }
        Ok(self.audio.as_mut().expect("audio player initialized"))
    }

    fn set_volume_percent(&mut self, volume_percent: u8) {
        self.volume_percent = volume_percent.min(MAX_VOLUME_PERCENT);
        if let Some(audio) = &self.audio {
            audio.set_volume(volume_multiplier(self.volume_percent));
        }
        self.config.volume_percent = self.volume_percent;
        if let Err(err) = self.config.save() {
            self.status = format!("Громкость изменена, но config не сохранён: {err}");
        }
    }

    fn update_system_media_state(&mut self) {
        let track = self.player.current_track_summary().cloned();
        let position = self.audio.as_ref().map(AudioPlayer::position);
        let duration = self.audio.as_ref().and_then(AudioPlayer::duration);
        if let Some(media_controls) = &mut self.media_controls {
            media_controls.set_track(track.as_ref(), duration, position, self.player.is_playing());
        }
    }

    fn player_bar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        let position = self
            .audio
            .as_ref()
            .map(AudioPlayer::position)
            .unwrap_or_default();
        let duration = self.audio.as_ref().and_then(AudioPlayer::duration);

        ui.horizontal_centered(|ui| {
            let track_text = current_track_bar_text(self.player.current_track());
            let controls_width = 44.0 + 52.0 + 44.0 + 72.0 + 44.0 + 36.0;
            let gaps_width = ui.spacing().item_spacing.x * 6.0;
            let capsule_width = (ui.available_width() - controls_width - gaps_width).max(220.0);
            if let Some(seek_position) =
                track_progress_capsule(ui, track_text, position, duration, capsule_width)
            {
                if let Some(audio) = &self.audio {
                    if let Err(err) = audio.seek(seek_position) {
                        self.status = err;
                    } else {
                        self.update_system_media_state();
                    }
                }
            }

            if ui
                .add_sized([44.0, 36.0], egui::Button::new("⏮"))
                .on_hover_text("Предыдущий трек")
                .clicked()
            {
                self.handle_player_command(PlayerCommand::Previous);
            }
            if ui
                .add_sized(
                    [52.0, 36.0],
                    egui::Button::new(play_pause_button_label(self.player.is_playing())),
                )
                .on_hover_text("Play/Pause")
                .clicked()
            {
                self.handle_player_command(PlayerCommand::PlayPause);
            }
            if ui
                .add_sized([44.0, 36.0], egui::Button::new("⏭"))
                .on_hover_text("Следующий трек")
                .clicked()
            {
                self.handle_player_command(PlayerCommand::Next);
            }
            if ui
                .add_sized([72.0, 36.0], egui::Button::new(self.playback_mode.label()))
                .on_hover_text("После конца трека: следующий или повтор текущего")
                .clicked()
            {
                self.playback_mode = self.playback_mode.toggle();
            }

            self.volume_menu(ui);
            self.account_menu(ui);
        });
        ui.add_space(10.0);
    }

    fn volume_menu(&mut self, ui: &mut egui::Ui) {
        let button =
            egui::Button::new(egui::RichText::new(volume_icon(self.volume_percent)).size(18.0))
                .min_size(egui::vec2(44.0, 36.0));
        let (response, _) = egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
            ui.set_min_width(56.0);
            ui.vertical_centered(|ui| {
                ui.label(format!("{}%", self.volume_percent));
                let mut volume = self.volume_percent;
                if ui
                    .add_sized(
                        [36.0, 128.0],
                        egui::Slider::new(&mut volume, 0..=150)
                            .vertical()
                            .show_value(false),
                    )
                    .changed()
                {
                    self.set_volume_percent(volume);
                }
            });
        });
        response.on_hover_text("Громкость приложения");
    }

    fn account_menu(&mut self, ui: &mut egui::Ui) {
        let Some(account) = self.account.clone() else {
            return;
        };

        let initials = account_initials(&account);
        let button = egui::Button::new(egui::RichText::new(initials).size(16.0))
            .min_size(egui::vec2(36.0, 36.0))
            .corner_radius(18.0);
        egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
            self.account_menu_contents(ui, &account);
        });
    }

    fn account_menu_contents(&mut self, ui: &mut egui::Ui, account: &AccountStatus) {
        ui.label(&account.display_name);
        ui.small(format!("uid: {}", account.uid));
        if account.display_name != account.login {
            ui.small(&account.login);
        }
        ui.separator();
        if ui.button("Сменить аккаунт").clicked() {
            self.switch_account();
            ui.close();
        }
    }

    fn switch_account(&mut self) {
        if let Some(audio) = &self.audio {
            audio.stop();
        }
        self.config.token = None;
        self.token_input.clear();
        self.account = None;
        self.favorites.clear();
        self.wave_stations.clear();
        self.active_wave_station = None;
        self.wave_tracks.clear();
        self.search_results.clear();
        self.player = PlayerState::default();
        self.queue_source = QueueSource::Favorites;
        self.loaded_audio_track_id = None;
        self.audio_busy = false;
        self.busy = false;

        match self.config.save() {
            Ok(()) => self.status = "Аккаунт сброшен. Войдите через Яндекс.".to_owned(),
            Err(err) => self.status = format!("Аккаунт сброшен, но config не сохранён: {err}"),
        }
        self.update_system_media_state();
    }
}

enum UiMessage {
    Audio(Result<(File, String, String), String>),
    Bootstrap(Result<BootstrapData, String>),
    Favorites(Result<Vec<TrackSummary>, String>),
    Search(Result<Vec<TrackSummary>, String>),
    Wave(Result<WaveData, String>, bool),
    TokenCaptured(Result<String, String>),
    OutputDeviceChanged(Option<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueueSource {
    Favorites,
    Search,
    Wave,
}

struct BootstrapData {
    account: AccountStatus,
    favorites: Result<Vec<TrackSummary>, String>,
    wave: Result<WaveData, String>,
}

struct WaveData {
    station: Option<WaveStation>,
    stations: Vec<WaveStation>,
    tracks: Vec<TrackSummary>,
    diagnostics: String,
}

fn load_wave_data(
    client: &YandexMusicClient<ReqwestHttpClient>,
    selected_station: Option<WaveStation>,
) -> Result<WaveData, crate::api::ApiError> {
    let stations = client.wave_stations()?;
    let station = selected_station
        .as_ref()
        .or_else(|| stations.iter().find(|station| station.tag == "onyourwave"))
        .or_else(|| stations.first());
    let Some(station) = station else {
        return Ok(WaveData {
            station: None,
            stations,
            tracks: Vec::new(),
            diagnostics: "stations empty".to_owned(),
        });
    };
    let station = station.clone();

    let station_label = format!("{}:{}", station.station_type, station.tag);
    match client.station_tracks(&station) {
        Ok(tracks) if !tracks.is_empty() => Ok(WaveData {
            station: Some(station),
            stations,
            diagnostics: format!("station {station_label}, station tracks ok"),
            tracks,
        }),
        Ok(_) => {
            let tracks = client.station_session_tracks(&station)?;
            Ok(WaveData {
                station: Some(station),
                stations,
                diagnostics: format!(
                    "station {station_label}, station tracks empty, session fallback ok"
                ),
                tracks,
            })
        }
        Err(station_err) => {
            let tracks = client.station_session_tracks(&station)?;
            Ok(WaveData {
                station: Some(station),
                stations,
                diagnostics: format!(
                    "station {station_label}, station tracks failed: {station_err}, session fallback ok"
                ),
                tracks,
            })
        }
    }
}

fn is_same_track(current: Option<&TrackSummary>, track: &TrackSummary) -> bool {
    current.is_some_and(|current| {
        current.id == track.id
            && (current.album_id == track.album_id
                || current.album_id.is_none()
                || track.album_id.is_none())
    })
}

fn account_initials(account: &AccountStatus) -> String {
    let source = if account.display_name.trim().is_empty() {
        account.login.as_str()
    } else {
        account.display_name.as_str()
    };
    let initials: String = source
        .split_whitespace()
        .filter_map(|part| part.chars().next())
        .take(2)
        .collect();

    if initials.is_empty() {
        "Я".to_owned()
    } else {
        initials.to_uppercase()
    }
}

fn play_pause_button_label(is_playing: bool) -> &'static str {
    if is_playing { "⏸" } else { "▶" }
}

fn current_track_bar_text(track: Option<&str>) -> &str {
    track.unwrap_or("Трек не выбран")
}

fn track_progress_capsule(
    ui: &mut egui::Ui,
    track_text: &str,
    position: Duration,
    duration: Option<Duration>,
    width: f32,
) -> Option<Duration> {
    let size = egui::vec2(width, 42.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let corner_radius = egui::CornerRadius::same(21);
    let base = egui::Color32::from_rgb(58, 58, 60);
    let progress = egui::Color32::from_rgb(9, 116, 146);

    painter.rect_filled(rect, corner_radius, base);

    if let Some(duration) = duration {
        let ratio = progress_ratio(position, duration);
        if ratio > 0.0 {
            let progress_rect = egui::Rect::from_min_max(
                rect.min,
                egui::pos2(rect.left() + rect.width() * ratio, rect.bottom()),
            );
            painter.rect_filled(progress_rect, corner_radius, progress);
        }
    }

    let text_color = egui::Color32::from_rgb(218, 218, 220);
    let time_text = duration
        .map(|duration| {
            format!(
                "{} / {}",
                format_duration(position),
                format_duration(duration)
            )
        })
        .unwrap_or_else(|| "--:--".to_owned());

    let title_rect =
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.right() - 150.0, rect.bottom()));
    let title_painter = ui.painter_at(title_rect);
    title_painter.text(
        title_rect.left_center() + egui::vec2(18.0, 0.0),
        egui::Align2::LEFT_CENTER,
        track_text,
        egui::FontId::proportional(18.0),
        text_color,
    );
    painter.text(
        rect.right_center() - egui::vec2(18.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        time_text,
        egui::FontId::proportional(14.0),
        text_color,
    );

    if (response.clicked() || response.dragged()) && duration.is_some() {
        response
            .interact_pointer_pos()
            .and_then(|pos| seek_position_from_x(rect.left(), rect.width(), pos.x, duration?))
    } else {
        None
    }
}

fn player_bar_height() -> f32 {
    PLAYER_BAR_HEIGHT
}

fn duration_seconds(duration: Duration) -> f32 {
    duration.as_secs_f32()
}

fn progress_ratio(position: Duration, duration: Duration) -> f32 {
    let duration = duration_seconds(duration);
    if duration <= 0.0 {
        return 0.0;
    }

    (duration_seconds(position) / duration).clamp(0.0, 1.0)
}

fn seek_position_from_x(
    rect_left: f32,
    rect_width: f32,
    pointer_x: f32,
    duration: Duration,
) -> Option<Duration> {
    if rect_width <= 0.0 {
        return None;
    }

    let ratio = ((pointer_x - rect_left) / rect_width).clamp(0.0, 1.0);
    Some(Duration::from_secs_f32(duration_seconds(duration) * ratio))
}

fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes}:{seconds:02}")
}

fn favorites_list_height(available_height: f32) -> f32 {
    available_height.max(160.0)
}

fn same_station(left: &WaveStation, right: &WaveStation) -> bool {
    left.station_type == right.station_type && left.tag == right.tag
}

fn volume_multiplier(volume_percent: u8) -> f32 {
    f32::from(volume_percent.min(MAX_VOLUME_PERCENT)) / 100.0
}

fn volume_icon(volume_percent: u8) -> &'static str {
    match volume_percent {
        0 => "🔇",
        1..=60 => "🔈",
        61..=100 => "🔉",
        _ => "🔊",
    }
}

fn should_pause_for_output_change(
    previous: Option<&str>,
    current: Option<&str>,
    is_playing: bool,
) -> bool {
    is_playing && previous != current
}

fn default_output_device_id() -> Option<String> {
    use rodio::cpal::traits::{DeviceTrait, HostTrait};

    rodio::cpal::default_host()
        .default_output_device()
        .and_then(|device| device.id().ok())
        .map(|id| id.1)
}

fn fetch_track_audio(token: String, track: &TrackSummary) -> Result<File, String> {
    let client = YandexMusicClient::new(token, ReqwestHttpClient::default());
    let playback_url = client
        .track_playback_url(&track.id, track.album_id.as_deref())
        .map_err(|err| err.to_string())?;
    let mut response = reqwest::blocking::get(playback_url.as_str())
        .map_err(|err| format!("audio download error: {err}"))?
        .error_for_status()
        .map_err(|err| format!("audio download error: {err}"))?;
    let mut file = create_audio_temp_file()?;

    std::io::copy(&mut response, &mut file)
        .map_err(|err| format!("audio download error: {err}"))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|err| format!("audio temp file error: {err}"))?;

    Ok(file)
}

fn create_audio_temp_file() -> Result<File, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("audio temp file error: {err}"))?
        .as_nanos();
    let path = std::env::temp_dir().join(format!("ya-player-{}-{now}.mp3", std::process::id()));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|err| format!("audio temp file error: {err}"))?;
    let _ = std::fs::remove_file(path);

    Ok(file)
}

fn current_browser_urls() -> Result<Vec<String>, String> {
    let scripts = [
        r#"tell application "Safari"
if it is running and (count of windows) > 0 then return URL of front document
end tell"#,
        r#"tell application "Google Chrome"
if it is running and (count of windows) > 0 then return URL of active tab of front window
end tell"#,
        r#"tell application "Yandex"
if it is running and (count of windows) > 0 then return URL of active tab of front window
end tell"#,
        r#"tell application "Yandex Browser"
if it is running and (count of windows) > 0 then return URL of active tab of front window
end tell"#,
    ];

    let mut urls = Vec::new();
    let mut errors = Vec::new();

    for script in scripts {
        match run_osascript(script) {
            Ok(url) if !url.is_empty() => urls.push(url),
            Ok(_) => {}
            Err(err) => errors.push(err),
        }
    }

    if urls.is_empty() && !errors.is_empty() && errors.iter().all(|err| is_permission_error(err)) {
        Err(errors.join("; "))
    } else {
        Ok(urls)
    }
}

fn run_osascript(script: &str) -> Result<String, String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|err| err.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn is_permission_error(error: &str) -> bool {
    error.contains("not allowed assistive access")
        || error.contains("Not authorized")
        || error.contains("not authorised")
        || error.contains("is not allowed")
        || error.contains("не разрешено")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_initials_use_display_name_words() {
        let account = AccountStatus {
            uid: 42,
            login: "demo-user".to_owned(),
            display_name: "Demo User".to_owned(),
        };

        assert_eq!(account_initials(&account), "DU");
    }

    #[test]
    fn account_initials_fallback_to_login() {
        let account = AccountStatus {
            uid: 42,
            login: "demo-user".to_owned(),
            display_name: "   ".to_owned(),
        };

        assert_eq!(account_initials(&account), "D");
    }

    #[test]
    fn play_pause_button_label_reflects_playback_state() {
        assert_eq!(play_pause_button_label(true), "⏸");
        assert_eq!(play_pause_button_label(false), "▶");
    }

    #[test]
    fn current_track_bar_text_falls_back_when_empty() {
        assert_eq!(
            current_track_bar_text(Some("Artist — Track")),
            "Artist — Track"
        );
        assert_eq!(current_track_bar_text(None), "Трек не выбран");
    }

    #[test]
    fn player_bar_has_fixed_compact_height() {
        assert_eq!(player_bar_height(), 72.0);
    }

    #[test]
    fn duration_formats_as_minutes_and_seconds() {
        assert_eq!(format_duration(Duration::from_secs(0)), "0:00");
        assert_eq!(format_duration(Duration::from_secs(65)), "1:05");
        assert_eq!(format_duration(Duration::from_secs(600)), "10:00");
    }

    #[test]
    fn progress_ratio_is_clamped() {
        assert_eq!(
            progress_ratio(Duration::from_secs(30), Duration::from_secs(120)),
            0.25
        );
        assert_eq!(
            progress_ratio(Duration::from_secs(180), Duration::from_secs(120)),
            1.0
        );
        assert_eq!(
            progress_ratio(Duration::from_secs(30), Duration::from_secs(0)),
            0.0
        );
    }

    #[test]
    fn seek_position_from_x_clamps_to_capsule_bounds() {
        let duration = Duration::from_secs(100);

        assert_eq!(
            seek_position_from_x(10.0, 200.0, 110.0, duration),
            Some(Duration::from_secs(50))
        );
        assert_eq!(
            seek_position_from_x(10.0, 200.0, -20.0, duration),
            Some(Duration::from_secs(0))
        );
        assert_eq!(
            seek_position_from_x(10.0, 200.0, 300.0, duration),
            Some(Duration::from_secs(100))
        );
        assert_eq!(seek_position_from_x(10.0, 0.0, 30.0, duration), None);
    }

    #[test]
    fn favorites_list_uses_available_height_with_floor() {
        assert_eq!(favorites_list_height(720.0), 720.0);
        assert_eq!(favorites_list_height(80.0), 160.0);
    }

    #[test]
    fn volume_percent_maps_to_audio_multiplier() {
        assert_eq!(volume_multiplier(100), 1.0);
        assert_eq!(volume_multiplier(50), 0.5);
        assert_eq!(volume_multiplier(200), 1.5);
    }

    #[test]
    fn volume_icon_reflects_volume_level() {
        assert_eq!(volume_icon(0), "🔇");
        assert_eq!(volume_icon(37), "🔈");
        assert_eq!(volume_icon(100), "🔉");
        assert_eq!(volume_icon(150), "🔊");
    }

    #[test]
    fn output_device_change_pauses_only_during_playback() {
        assert!(should_pause_for_output_change(
            Some("AirPods"),
            Some("MacBook Speakers"),
            true
        ));
        assert!(!should_pause_for_output_change(
            Some("AirPods"),
            Some("MacBook Speakers"),
            false
        ));
        assert!(!should_pause_for_output_change(
            Some("AirPods"),
            Some("AirPods"),
            true
        ));
    }
}
