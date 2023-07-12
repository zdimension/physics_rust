use bevy::math::Vec2;
use bevy_egui::egui;
use bevy_egui::egui::{pos2, vec2, Response, Sense, TextureId, Ui, Widget, WidgetInfo, WidgetType, WidgetText, NumExt, TextStyle};

pub struct TextButton {
    text: WidgetText,
    selected: bool,
}

impl TextButton {
    pub fn new(text: impl Into<WidgetText>) -> Self {
        Self {
            text: text.into(),
            selected: false,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl Widget for TextButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self { text, selected } = self;
        let button_padding = ui.spacing().button_padding;
        let text_wrap_width = ui.available_width() - button_padding.x * 2.0;

        let text = text.into_galley(ui, Some(false), text_wrap_width, TextStyle::Button);
        let mut desired_size = text.size();
        desired_size.y = desired_size.y.at_least(ui.spacing().interact_size.y);
        desired_size += button_padding * 2.0;

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

            let text_pos = pos2(
                rect.min.x + button_padding.x,
                rect.center().y - text.size().y / 2.0,
            );
            text.paint_with_visuals(ui.painter(), text_pos, visuals);
        }

        response
    }
}
