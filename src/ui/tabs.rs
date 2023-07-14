use std::ops::DerefMut;
use bevy_egui::egui::{NumExt, pos2, Response, Sense, TextStyle, Ui, Widget, WidgetInfo, WidgetText, WidgetType};
use strum::IntoEnumIterator;

pub trait Tab: PartialEq + Copy + IntoEnumIterator + Send + Sync + 'static {
    fn name(&self) -> &str;
}

struct TabButton {
    text: WidgetText,
    selected: bool
}

impl Widget for TabButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self {
            text,
            selected,
        } = self;
        let button_padding = ui.spacing().button_padding;
        let text_wrap_width = ui.available_width() - button_padding.x * 2.0;

        let text = text.into_galley(ui, Some(false), text_wrap_width, TextStyle::Button);
        let mut desired_size = text.size();
        desired_size.y = desired_size.y.at_least(ui.spacing().interact_size.y);
        desired_size += button_padding * 2.0;

        let (rect, response) = ui.allocate_at_least(desired_size, Sense::click());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, text.text()));

        if ui.is_rect_visible(rect) {
            let selection = ui.visuals().widgets.hovered;
            if response.hovered() || selected {
                ui.painter().rect(
                    rect.expand(selection.expansion),
                    selection.rounding,
                    selection.bg_fill,
                    selection.bg_stroke,
                );
            }

            let text_pos = {
                pos2(
                    rect.min.x + button_padding.x,
                    rect.center().y - text.size().y / 2.0,
                )
            };
            text.paint_with_visuals(ui.painter(), text_pos, &selection);
        }

        response
    }
}

pub fn tabs<T: Tab>(ui: &mut Ui, mut current_tab: impl DerefMut<Target = T>, tab: impl FnOnce(&mut Ui, T)) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            for tab in T::iter() {
                if ui.add(TabButton { text: tab.name().into(), selected: tab == *current_tab }).clicked() {
                    *current_tab = tab;
                }
            }
        });

        tab(ui, *current_tab);
    });
}