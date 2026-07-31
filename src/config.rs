use bevy::prelude::Resource;

#[derive(Resource, Copy, Clone)]
pub struct AppConfig {
    pub ui_scale: i32,
    pub zoom_speed: f32,
    pub tool_cursor: bool,
    pub kinetic_panning: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            ui_scale: 100,
            zoom_speed: 1.0,
            tool_cursor: true,
            kinetic_panning: true,
        }
    }
}

impl AppConfig {
    pub fn ui_scale_factor(&self) -> f32 {
        self.ui_scale as f32 / 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::AppConfig;

    #[test]
    fn converts_menu_percentage_to_a_fractional_scale() {
        let config = AppConfig {
            ui_scale: 83,
            ..Default::default()
        };

        assert!((config.ui_scale_factor() - 0.83).abs() < f32::EPSILON);
    }
}
