use bevy::prelude::Component;
use crate::egui_systems;

#[derive(Default, Component)]
pub struct ControllerWindow;

egui_systems!();