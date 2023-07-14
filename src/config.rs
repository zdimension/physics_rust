use bevy::prelude::Resource;

#[derive(Resource, Copy, Clone)]
pub struct AppConfig {
    pub ui_scale: i32,
    pub zoom_speed: f32,
    pub tool_cursor: bool,
    pub kinetic_panning: bool
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            ui_scale: 100,
            zoom_speed: 1.0,
            tool_cursor: true,
            kinetic_panning: true
        }
    }
}