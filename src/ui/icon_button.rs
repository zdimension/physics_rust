use std::ops::AddAssign;
use bevy::math::Vec2;
use bevy_egui::egui;
use bevy_egui::egui::{pos2, Response, Sense, TextureId, Ui, Widget, WidgetInfo, WidgetType, Color32};
use num_traits::Pow;

pub struct IconButton {
    icon: egui::widgets::Image,
    selected: bool,
    dim_if_unselected: bool
}

impl IconButton {
    pub fn new(icon: TextureId, size: f32) -> Self {
        Self {
            icon: egui::widgets::Image::new(icon, Vec2::splat(size).to_array()),
            selected: false,
            dim_if_unselected: false
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn dim_if_unselected(mut self, dim: bool) -> Self {
        self.dim_if_unselected = dim;
        self
    }
}

impl Widget for IconButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self { icon, selected, dim_if_unselected } = self;
        let desired_size = icon.size();// + vec2(2.0, 2.0);
        let full_factor = 2f32.pow(desired_size.x / 8.0 - 2.0);
        let full_size = desired_size + egui::Vec2::splat(2.0 * full_factor);

        let (rect, response) = ui.allocate_exact_size(full_size, Sense::click());
        assert_eq!(rect.size(), full_size);
        response.widget_info(|| WidgetInfo::new(WidgetType::ImageButton));

        if ui.is_rect_visible(rect) {
            let real_rect = rect.shrink(full_factor);
            let visuals = ui.style().interact(&response);

            if response.hovered() {
                ui.painter().rect(
                    real_rect.expand(full_factor),
                    visuals.rounding,
                    visuals.bg_fill,
                    visuals.bg_stroke,
                );
            }
            if selected {
                let selection = ui.visuals().selection;
                ui.painter().rect(
                    real_rect.expand(1.0),
                    visuals.rounding,
                    selection.bg_fill,
                    selection.stroke,
                );
            }

            assert_eq!(icon.size(), real_rect.size());

            let image_rect =
                egui::Rect::from_min_size(pos2(real_rect.min.x, real_rect.min.y), icon.size());
            let icon = if !selected && dim_if_unselected {
                icon.tint(Color32::from_gray(180))
            } else { icon };
            icon.paint_at(ui, image_rect);
        }

        response
    }
}
