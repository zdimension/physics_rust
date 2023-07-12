use crate::ui::windows::scene::background::BackgroundWindow;
use bevy::prelude::{App, Update};

pub mod background;

pub fn add_ui_systems(app: &mut App) {
    app.add_systems(Update, BackgroundWindow::show);
}
