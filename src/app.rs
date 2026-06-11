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
use crate::player::{PlaybackMode, PlayerCommand, PlayerState, Shortcut};

const PLAYER_BAR_HEIGHT: f32 = 64.0;
const MAX_VOLUME_PERCENT: u8 = 150;

pub struct YaPlayerApp {
    config: AppConfig,
    token_input: String,
    status: String,
    account: Option<AccountStatus>,
    favorites: Vec<TrackSummary>,
    wave_stations: Vec<WaveStation>,
    wave_tracks: Vec<TrackSummary>,
    player: PlayerState,
    queue_source: QueueSource,
    playback_mode: PlaybackMode,
    audio: Option<AudioPlayer>,
    volume_percent: u8,
    loaded_audio_track_id: Option<String>,
    output_device_id: Option<String>,
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
        let (hotkeys, hotkey_status) = match MediaHotkeys::register() {
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
        };

        let mut app = Self {
            config,
            token_input,
            status: hotkey_status,
            account: None,
            favorites: Vec::new(),
            wave_stations: Vec::new(),
            wave_tracks: Vec::new(),
            player: PlayerState::default(),
            queue_source: QueueSource::Favorites,
            playback_mode: PlaybackMode::Continue,
            audio: None,
            volume_percent,
            loaded_audio_track_id: None,
            output_device_id: default_output_device_id(),
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
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Ya Player").strong());
                ui.separator();
                ui.label(egui::RichText::new(&self.status).small());
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
            }

            ui.separator();
            let current_track = self.player.current_track_summary().cloned();
            let favorite_rows = self.favorites.clone();
            let mut favorite_clicked = None;
            let mut load_favorites_clicked = false;
            let mut play_wave_clicked = false;

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
                            if ui.selectable_label(selected, row).clicked() {
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
                        "Нажмите play, чтобы запустить волну."
                    } else {
                        "Волна готова. Нажмите play."
                    };
                columns[1].label(wave_label);
                columns[1].add_space(8.0);
                if columns[1]
                    .add_enabled(
                        !self.busy,
                        egui::Button::new("▶ Play").min_size([120.0, 36.0].into()),
                    )
                    .on_hover_text("Запустить волну")
                    .clicked()
                {
                    play_wave_clicked = true;
                }
            });

            if load_favorites_clicked {
                self.load_favorites();
            }
            if play_wave_clicked {
                self.play_wave();
            }
            if let Some(index) = favorite_clicked {
                self.select_favorite_track(index);
            }
        });
    }
}

impl YaPlayerApp {
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
                                }
                                Err(err) => {
                                    self.player.pause();
                                    self.status = err;
                                }
                            },
                            Err(err) => {
                                self.player.pause();
                                self.status = err;
                            }
                        },
                        Err(err) => {
                            self.player.pause();
                            self.status = err;
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
                            self.wave_stations = data.stations;
                            self.wave_tracks = data.tracks;
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
            let wave = load_wave_data(&client).map_err(|err| err.to_string());

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
        let Some(token) = self.valid_token() else {
            return;
        };
        self.spawn_request("Загружаю волну...", move || {
            let client = YandexMusicClient::new(token, ReqwestHttpClient::default());
            UiMessage::Wave(
                load_wave_data(&client).map_err(|err| err.to_string()),
                autoplay,
            )
        });
    }

    fn select_favorite_track(&mut self, index: usize) {
        self.select_track_from_list(self.favorites.clone(), index);
    }

    fn play_wave(&mut self) {
        if self.wave_tracks.is_empty() {
            self.load_wave(true);
            return;
        }

        self.select_track_from_source(self.wave_tracks.clone(), 0, QueueSource::Wave);
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
                } else if self.can_resume_current_track() {
                    let audio = self.audio.as_ref().expect("checked audio");
                    audio.resume();
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
                if should_continue {
                    self.play_current_track();
                }
            }
            PlayerCommand::Previous => {
                let should_continue = self.player.is_playing();
                self.player.apply(command);
                if should_continue {
                    self.play_current_track();
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

    fn player_bar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal_centered(|ui| {
            let track_text = current_track_bar_text(self.player.current_track());
            let title_width = (ui.available_width() - 380.0).max(180.0);
            ui.add_sized(
                [title_width, 40.0],
                egui::Label::new(egui::RichText::new(track_text).size(16.0)).truncate(),
            );

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
                .add_sized(
                    [116.0, 36.0],
                    egui::Button::new(format!("Режим: {}", self.playback_mode.label())),
                )
                .on_hover_text("После конца трека: следующий или повтор текущего")
                .clicked()
            {
                self.playback_mode = self.playback_mode.toggle();
            }

            self.volume_menu(ui);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.account_menu(ui);
            });
        });
        ui.add_space(6.0);
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
        self.wave_tracks.clear();
        self.player = PlayerState::default();
        self.queue_source = QueueSource::Favorites;
        self.loaded_audio_track_id = None;
        self.audio_busy = false;
        self.busy = false;

        match self.config.save() {
            Ok(()) => self.status = "Аккаунт сброшен. Войдите через Яндекс.".to_owned(),
            Err(err) => self.status = format!("Аккаунт сброшен, но config не сохранён: {err}"),
        }
    }
}

enum UiMessage {
    Audio(Result<(File, String, String), String>),
    Bootstrap(Result<BootstrapData, String>),
    Favorites(Result<Vec<TrackSummary>, String>),
    Wave(Result<WaveData, String>, bool),
    TokenCaptured(Result<String, String>),
    OutputDeviceChanged(Option<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueueSource {
    Favorites,
    Wave,
}

struct BootstrapData {
    account: AccountStatus,
    favorites: Result<Vec<TrackSummary>, String>,
    wave: Result<WaveData, String>,
}

struct WaveData {
    stations: Vec<WaveStation>,
    tracks: Vec<TrackSummary>,
    diagnostics: String,
}

fn load_wave_data(
    client: &YandexMusicClient<ReqwestHttpClient>,
) -> Result<WaveData, crate::api::ApiError> {
    let stations = client.wave_stations()?;
    let Some(station) = stations
        .iter()
        .find(|station| station.tag == "onyourwave")
        .or_else(|| stations.first())
    else {
        return Ok(WaveData {
            stations,
            tracks: Vec::new(),
            diagnostics: "stations empty".to_owned(),
        });
    };

    let station_label = format!("{}:{}", station.station_type, station.tag);
    match client.station_tracks(station) {
        Ok(tracks) if !tracks.is_empty() => Ok(WaveData {
            stations,
            diagnostics: format!("station {station_label}, station tracks ok"),
            tracks,
        }),
        Ok(_) => {
            let tracks = client.station_session_tracks(station)?;
            Ok(WaveData {
                stations,
                diagnostics: format!(
                    "station {station_label}, station tracks empty, session fallback ok"
                ),
                tracks,
            })
        }
        Err(station_err) => {
            let tracks = client.station_session_tracks(station)?;
            Ok(WaveData {
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

fn player_bar_height() -> f32 {
    PLAYER_BAR_HEIGHT
}

fn favorites_list_height(available_height: f32) -> f32 {
    available_height.max(160.0)
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
        assert_eq!(player_bar_height(), 64.0);
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
