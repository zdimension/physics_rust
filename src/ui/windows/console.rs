use bevy::prelude::{App, ButtonInput, IntoScheduleConfigs, KeyCode, Res, ResMut};
use bevy_egui::egui::{self, Key, KeyboardShortcut, Modifiers, TextEdit, TextStyle};
use bevy_egui::egui::text::{CCursor, CCursorRange};
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};
use egui_extras::syntax_highlighting::{code_view_ui, highlight, CodeTheme};

use crate::script::thyme::{Console, execute_console};
use crate::ui::WindowExt;

pub fn add_systems(app: &mut App) {
    app.add_systems(
        EguiPrimaryContextPass,
        (toggle_console, draw_console, execute_console).chain(),
    );
}

fn toggle_console(keys: Res<ButtonInput<KeyCode>>, mut console: ResMut<Console>) {
    if keys.just_pressed(KeyCode::F10) {
        console.open = !console.open;
    }
}

pub fn draw_console(
    mut egui_ctx: EguiContexts,
    mut console: ResMut<Console>,
) {
    if !console.open {
        return;
    }
    let ctx = egui_ctx.ctx_mut().expect("primary egui context");
    let mut open = true;
    egui::Window::new("Console")
        .open(&mut open)
        .default_size([600.0, 300.0])
        .show_translucent(ctx, |ui| {
            let theme = CodeTheme::from_memory(ui.ctx(), ui.style());
            let background = if theme.is_dark() {
                egui::Color32::BLACK
            } else {
                egui::Color32::WHITE
            };
            let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, width: f32| {
                let mut job = highlight(ui.ctx(), ui.style(), &theme, text.as_str(), "rs");
                job.wrap.max_width = width;
                ui.fonts_mut(|fonts| fonts.layout_job(job))
            };
            let input_rows = console.input.split('\n').count().min(6);
            let input_height = ui.text_style_height(&TextStyle::Monospace) * input_rows as f32 + 8.0;
            let output_height =
                (ui.available_height() - input_height - ui.spacing().item_spacing.y).max(80.0);
            egui::Frame::new().fill(background).show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .min_scrolled_height(output_height)
                    .max_height(output_height)
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        code_view_ui(ui, &theme, &console.output, "rs");
                    });
            });
            let input_id = ui.make_persistent_id("input");
            let mut edit_state = TextEdit::load_state(ui.ctx(), input_id).unwrap_or_default();
            let caret = edit_state
                .cursor
                .char_range()
                .map(|range| range.primary.index.0)
                .unwrap_or_else(|| console.input.chars().count());
            let has_line_above = console.input.chars().take(caret).any(|ch| ch == '\n');
            let has_line_below = console.input.chars().skip(caret).any(|ch| ch == '\n');
            let focused = ui.memory(|memory| memory.has_focus(input_id));
            let history_changed = if focused
                && !has_line_above
                && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowUp))
            {
                console.history_up()
            } else if focused
                && !has_line_below
                && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowDown))
            {
                console.history_down()
            } else {
                false
            };
            if history_changed {
                edit_state
                    .cursor
                    .set_char_range(Some(CCursorRange::one(CCursor::new(
                        console.input.chars().count(),
                    ))));
                TextEdit::store_state(ui.ctx(), input_id, edit_state);
            }
            let edit = TextEdit::multiline(&mut console.input)
                .id(input_id)
                .code_editor()
                .desired_rows(input_rows)
                .desired_width(f32::INFINITY)
                .return_key(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter))
                .background_color(background)
                .layouter(&mut layouter);
            let response = egui::ScrollArea::vertical()
                .min_scrolled_height(input_height)
                .max_height(input_height)
                .auto_shrink([false, false])
                .show(ui, |ui| edit.show(ui).response)
                .inner;
            if response.has_focus()
                && ui.input(|input| input.key_pressed(Key::Enter) && !input.modifiers.shift)
            {
                console.submit();
                response.request_focus();
            }
        });
    console.open = open;
}
