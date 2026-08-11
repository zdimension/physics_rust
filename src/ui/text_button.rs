use bevy_egui::egui::{
    NumExt, Response, Sense, TextStyle, TextWrapMode, Ui, Widget, WidgetInfo, WidgetText,
    WidgetType, pos2,
};

pub struct TextButton {
    text: WidgetText,
}

impl TextButton {
    pub fn new(text: impl Into<WidgetText>) -> Self {
        Self { text: text.into() }
    }
}

impl Widget for TextButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self { text } = self;
        let button_padding = ui.spacing().button_padding;
        let text_wrap_width = ui.available_width() - button_padding.x * 2.0;

        let text = text.into_galley(
            ui,
            Some(TextWrapMode::Extend),
            text_wrap_width,
            TextStyle::Button,
        );
        let mut desired_size = text.size();
        desired_size.y = desired_size.y.at_least(ui.spacing().interact_size.y);
        desired_size += button_padding * 2.0;

        let (rect, response) = ui.allocate_at_least(desired_size, Sense::click());
        response
            .widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), text.text()));

        if ui.is_rect_visible(rect) {
            let visuals = ui.style().interact(&response);

            if response.hovered() {
                ui.painter().rect(
                    rect.expand(visuals.expansion),
                    visuals.corner_radius,
                    visuals.bg_fill,
                    visuals.bg_stroke,
                    bevy_egui::egui::StrokeKind::Outside,
                );
            }
            let text_pos = pos2(
                rect.min.x + button_padding.x,
                rect.center().y - text.size().y / 2.0,
            );
            ui.painter().galley(text_pos, text, visuals.text_color());
        }

        response
    }
}
