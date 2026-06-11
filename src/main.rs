use ya_player::app::YaPlayerApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([860.0, 560.0])
            .with_min_inner_size([720.0, 460.0])
            .with_icon(app_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "Ya Player",
        options,
        Box::new(|_cc| Ok(Box::<YaPlayerApp>::default())),
    )
}

fn app_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/YaPlayer.png"))
        .expect("embedded app icon must be a valid PNG")
}
