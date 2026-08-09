use bevy::prelude::Resource;

#[derive(Resource, Copy, Clone)]
pub struct AppConfig {
    pub ui_scale: f32,
    pub zoom_speed: f32,
    pub tool_cursor: bool,
    pub kinetic_panning: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            zoom_speed: 1.0,
            tool_cursor: true,
            kinetic_panning: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppConfig;

    #[test]
    fn stores_ui_scale_as_a_factor() {
        let config = AppConfig {
            ui_scale: 0.83,
            ..Default::default()
        };

        assert!((config.ui_scale - 0.83).abs() < f32::EPSILON);
    }
}
