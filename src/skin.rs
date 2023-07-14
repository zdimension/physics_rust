use bevy::prelude::*;
use bevy_egui::egui;
use bevy_egui::egui::ecolor::HsvaGamma;
use serde;

#[derive(Debug, Copy, Clone)]
pub struct Skin {
    pub accent: egui::Color32,
    pub opacity: f32
}

impl Default for Skin {
    fn default() -> Self {
        Self {
            accent: HsvaGamma { h: 210.0 / 360.0, s: 0.09, v: 0.57, a: 1.0 }.into(),
            opacity: 0.98
        }
    }
}

#[derive(Resource, Default)]
pub struct SkinConfig {
    pub current_skin: Skin,
}
