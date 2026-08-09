use bevy::prelude::{App, IntoScheduleConfigs, ResMut};
use bevy_egui::egui::{self, Key, KeyboardShortcut, Modifiers, TextEdit, TextStyle};
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};
use egui_extras::syntax_highlighting::{code_view_ui, highlight, CodeTheme};

use crate::script::thyme::{Console, execute_console};
use crate::ui::WindowExt;

pub fn add_systems(app: &mut App) {
    app.add_systems(
        EguiPrimaryContextPass,
        (draw_console, execute_console.after(draw_console)),
    );
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
            let input_height = ui.text_style_height(&TextStyle::Monospace) + 8.0;
            let output_height = (ui.available_height() - input_height).max(80.0);
            egui::ScrollArea::vertical()
                .min_scrolled_height(output_height)
                .max_height(output_height)
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    code_view_ui(ui, &theme, &console.output, "rs");
                });
            let response = ui.add(
                TextEdit::multiline(&mut console.input)
                    .code_editor()
                    .desired_rows(1)
                    .desired_width(f32::INFINITY)
                    .return_key(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter))
                    .background_color(background)
                    .layouter(&mut layouter),
            );
            if response.has_focus()
                && ui.input(|input| input.key_pressed(Key::Enter) && !input.modifiers.shift)
            {
                console.submit();
                response.request_focus();
            }
        });
    console.open = open;
}
