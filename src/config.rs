use bevy::prelude::{Color, Resource};

#[derive(Resource, Copy, Clone)]
pub struct AppConfig {
    pub ui_scale: f32,
    pub zoom_speed: f32,
    pub tool_cursor: bool,
    pub kinetic_panning: bool,
    pub laser_width: f32,
    pub angle_color: Color,
    pub polytool_preview_color: Color,
    pub enable_script_menu: bool,
    pub draw_scale_indicator: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            zoom_speed: 1.0,
            tool_cursor: true,
            kinetic_panning: true,
            laser_width: 0.2,
            angle_color: Color::srgba(1.0, 0.25, 1.0, 0.5),
            polytool_preview_color: Color::srgba(1.0, 0.5, 1.0, 0.4),
            enable_script_menu: true,
            draw_scale_indicator: true,
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
