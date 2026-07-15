use bevy::prelude::Component;
use crate::egui_systems;

#[derive(Default, Component)]
pub struct TextWindow;

egui_systems!();