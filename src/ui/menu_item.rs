use bevy::math::Vec2;
use bevy_egui::egui;
use bevy_egui::egui::{
    pos2, vec2, NumExt, Response, Sense, TextStyle, TextureId, Ui, Widget, WidgetInfo, WidgetText,
    WidgetType,
};

pub struct MenuItem {
    icon: Option<egui::widgets::Image>,
    text: WidgetText,
    icon_right: Option<egui::widgets::Image>,
    selected: bool,
}

impl MenuItem {
    const ICON_SIZE: f32 = 16.0;

    fn gen_image(icon: TextureId) -> egui::widgets::Image {
        egui::widgets::Image::new(icon, Vec2::splat(Self::ICON_SIZE).to_array())
    }

    pub fn button(icon: Option<TextureId>, text: impl Into<WidgetText>) -> Self {
        Self {
            icon: icon.map(Self::gen_image),
            text: text.into(),
            icon_right: None,
            selected: false,
        }
    }

    pub fn menu(icon: Option<TextureId>, text: impl Into<WidgetText>, icon_right: TextureId) -> Self {
        Self {
            icon_right: Some(Self::gen_image(icon_right)),
            ..Self::button(icon, text)
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl Widget for MenuItem {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self {
            icon,
            text,
            icon_right,
            selected,
        } = self;
        let button_padding = ui.spacing().button_padding;
        let icon_count = 1 + icon_right.is_some() as usize;
        let icon_width = Self::ICON_SIZE * ui.spacing().icon_spacing;
        let icon_width_total = icon_width * icon_count as f32;
        let text_wrap_width = ui.available_width() - button_padding.x * 2.0 - icon_width_total;

        let text = text.into_galley(ui, Some(false), text_wrap_width, TextStyle::Button);
        let mut desired_size = text.size();
        desired_size.x += icon_width_total;
        desired_size.y = desired_size.y.max(Self::ICON_SIZE);
        desired_size.y = desired_size.y.at_least(ui.spacing().interact_size.y);
        desired_size += button_padding * 2.0;

        desired_size.x = desired_size.x.at_least(ui.available_width());

        let (rect, response) = ui.allocate_at_least(desired_size, Sense::click());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, text.text()));

        if ui.is_rect_visible(rect) {
            let visuals = ui.style().interact(&response);

            if response.hovered() {
                ui.painter().rect(
                    rect.expand(visuals.expansion),
                    visuals.rounding,
                    visuals.bg_fill,
                    visuals.bg_stroke,
                );
            }
            if selected {
                let selection = ui.visuals().selection;
                ui.painter().rect(
                    rect.expand(visuals.expansion),
                    visuals.rounding,
                    selection.bg_fill,
                    selection.stroke,
                );
            }

            let text_pos = {
                let icon_spacing = ui.spacing().icon_spacing;
                pos2(
                    rect.min.x + button_padding.x + Self::ICON_SIZE + icon_spacing,
                    rect.center().y - text.size().y / 2.0,
                )
            };
            text.paint_with_visuals(ui.painter(), text_pos, visuals);

            if let Some(icon) = icon {
                let image_rect = egui::Rect::from_min_size(
                    pos2(rect.min.x, rect.center().y - 0.5 - (Self::ICON_SIZE / 2.0)),
                    vec2(Self::ICON_SIZE, Self::ICON_SIZE),
                );
                icon.paint_at(ui, image_rect);
            }

            if let Some(icon) = icon_right {
                let image_rect = egui::Rect::from_min_size(
                    pos2(
                        rect.max.x - Self::ICON_SIZE,
                        rect.center().y - 0.5 - (Self::ICON_SIZE / 2.0),
                    ),
                    vec2(Self::ICON_SIZE, Self::ICON_SIZE),
                );
                icon.paint_at(ui, image_rect);
            }
        }

        response
    }
}
