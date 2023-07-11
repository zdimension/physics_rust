use bevy::prelude::App;
use crate::ui::windows::scene::background::BackgroundWindow;

pub mod background;

pub fn add_ui_systems(app: &mut App) {
    app.add_system((BackgroundWindow::show));
}