use bevy_egui::egui::{Response, Sense, Ui, vec2, Widget};

pub struct SeparatorCustom {
    spacing: f32,
    is_horizontal_line: Option<bool>,
}

impl Default for SeparatorCustom {
    fn default() -> Self {
        Self {
            spacing: 6.0,
            is_horizontal_line: None,
        }
    }
}

impl SeparatorCustom {
    /// How much space we take up. The line is painted in the middle of this.
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    /// Explicitly ask for a horizontal line.
    /// By default you will get a horizontal line in vertical layouts,
    /// and a vertical line in horizontal layouts.
    pub fn horizontal(mut self) -> Self {
        self.is_horizontal_line = Some(true);
        self
    }

    /// Explicitly ask for a vertical line.
    /// By default you will get a horizontal line in vertical layouts,
    /// and a vertical line in horizontal layouts.
    pub fn vertical(mut self) -> Self {
        self.is_horizontal_line = Some(false);
        self
    }
}

impl Widget for SeparatorCustom {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self {
            spacing,
            is_horizontal_line,
        } = self;

        let is_horizontal_line =
            is_horizontal_line.unwrap_or_else(|| !ui.layout().main_dir().is_horizontal());

        let available_space = ui.min_size();

        let size = if is_horizontal_line {
            vec2(available_space.x, spacing)
        } else {
            vec2(spacing, available_space.y)
        };

        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());

        if ui.is_rect_visible(response.rect) {
            let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
            let painter = ui.painter();
            if is_horizontal_line {
                painter.hline(
                    rect.x_range(),
                    painter.round_to_pixel(rect.center().y),
                    stroke,
                );
            } else {
                painter.vline(
                    painter.round_to_pixel(rect.center().x),
                    rect.y_range(),
                    stroke,
                );
            }
        }

        response
    }
}
