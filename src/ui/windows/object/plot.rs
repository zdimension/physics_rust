use crate::measures::{AggregateMeasureData, AggregateMeasures, aggregate_measures};
use crate::ui::images::GuiIcons;
use crate::ui::{
    InitialPos, Subwindow, WindowSelectionTarget, bool_checkbox, window_target_entities,
};
use avian2d::prelude::*;
use bevy::prelude::ChildOf;
use bevy::prelude::{
    App, Commands, Component, Entity, FixedPostUpdate, IntoScheduleConfigs, Query, Res, Time,
};
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use egui::containers::PopupCloseBehavior;
use egui::containers::menu::{MenuButton, MenuConfig};
use egui::load::SizedTexture;
use egui_plot::{HoverPosition, Line, Plot, PlotPoint, PlotPoints};
use itertools::Itertools;
use std::fmt::{Display, Formatter};

pub fn add_systems(app: &mut App) {
    app.add_systems(EguiPrimaryContextPass, PlotWindow::show)
        .add_systems(
            FixedPostUpdate,
            PlotWindow::sample.after(PhysicsSystems::Writeback),
        );
}

struct AxisSetting {
    category: &'static [PlotQuantity],
    measures: Vec<&'static PlotQuantity>,
}

#[derive(Component)]
pub struct PlotWindow {
    quantities: Vec<(&'static [PlotQuantity], Vec<&'static PlotQuantity>)>,
    series: Vec<PlotSeries>,
    x: AxisSetting,
    y: AxisSetting,
    time: f32,
    time_span: f32,
    show_axes: bool,
    sidebar: bool,
}

struct PlotSeriesId {
    name: String,
    x: &'static PlotQuantity,
    y: &'static PlotQuantity,
}

impl PlotSeriesId {
    fn new(x: &'static PlotQuantity, y: &'static PlotQuantity) -> Self {
        Self {
            name: format!("{} / {}", y.name, x.name),
            x,
            y,
        }
    }
}

impl Display for PlotSeriesId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

struct PlotSeries {
    id: PlotSeriesId,
    sample_times: Vec<f32>,
    points: Vec<PlotPoint>,
    visible: bool,
}

impl PlotSeries {
    fn new(x: &'static PlotQuantity, y: &'static PlotQuantity) -> Self {
        Self {
            id: PlotSeriesId::new(x, y),
            sample_times: Vec::new(),
            points: Vec::new(),
            visible: true,
        }
    }

    fn push(&mut self, time: f32, point: PlotPoint) {
        self.sample_times.push(time);
        self.points.push(point);
    }

    fn clear(&mut self) {
        self.sample_times.clear();
        self.points.clear();
    }

    fn points_in_time_span(&self, current_time: f32, time_span: f32) -> &[PlotPoint] {
        debug_assert_eq!(self.sample_times.len(), self.points.len());
        let start = if time_span.is_finite() {
            let cutoff = current_time - time_span;
            self.sample_times.partition_point(|time| *time < cutoff)
        } else {
            0
        };
        &self.points[start..]
    }
}

fn series_color(index: usize) -> egui::Color32 {
    let golden_ratio = (5.0_f32.sqrt() - 1.0) / 2.0;
    egui::ecolor::Hsva::new(index as f32 * golden_ratio, 0.85, 0.5, 1.0).into()
}

type QuantityFn = fn(f32, &AggregateMeasures) -> Option<f32>;

struct PlotQuantity {
    name: &'static str,
    measure: QuantityFn,
}

impl Display for PlotQuantity {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

const fn quantity(name: &'static str, measure: QuantityFn) -> PlotQuantity {
    PlotQuantity { name, measure }
}

static PLOT_QUANTITIES: &[&[PlotQuantity]] = &[
    &[quantity("Time", |time, _| Some(time))],
    &[
        quantity("Position (x)", |_, query| Some(query.position?.x)),
        quantity("Position (y)", |_, query| Some(query.position?.y)),
    ],
    &[
        quantity("Speed", |_, query| Some(query.velocity?.length())),
        quantity("Velocity (x)", |_, query| Some(query.velocity?.x)),
        quantity("Velocity (y)", |_, query| Some(query.velocity?.y)),
    ],
    &[quantity("Angular velocity", |_, query| {
        query.angular_velocity
    })],
    // todo: acceleration
    // todo: force
    &[
        quantity("Momentum (x)", |_, query| Some(query.momentum?.linear.x)),
        quantity("Momentum (y)", |_, query| Some(query.momentum?.linear.y)),
    ],
    &[quantity("Angular momentum", |_, query| {
        Some(query.momentum?.angular)
    })],
    &[
        quantity("Linear kinetic energy", |_, query| query.kinetic_linear),
        quantity("Angular kinetic energy", |_, query| query.kinetic_angular),
        quantity("Kinetic energy (sum)", |_, query| query.kinetic_total()),
        quantity("Potential gravitational energy", |_, query| {
            query.gravity_energy
        }),
        quantity("Potential spring energy", |_, query| query.spring_energy),
        quantity("Potential energy (sum)", |_, query| query.potential_total()),
        quantity("Energy (sum)", |_, query| query.energy_total()),
    ],
];

impl Default for PlotWindow {
    fn default() -> Self {
        Self {
            quantities: vec![],
            series: vec![PlotSeries::new(
                &PLOT_QUANTITIES[0][0],
                &PLOT_QUANTITIES[2][0],
            )],
            x: AxisSetting {
                category: PLOT_QUANTITIES[0],
                measures: vec![&PLOT_QUANTITIES[0][0]],
            },
            y: AxisSetting {
                category: PLOT_QUANTITIES[2],
                measures: vec![&PLOT_QUANTITIES[2][0]],
            },
            time: 0.0,
            time_span: 30.0,
            show_axes: true,
            sidebar: true,
        }
    }
}

impl PlotWindow {
    fn sample(
        mut plots: Query<(
            Option<&ChildOf>,
            Option<&WindowSelectionTarget>,
            &mut PlotWindow,
        )>,
        ents: Query<AggregateMeasureData>,
        body_positions: Query<(&Position, &Rotation)>,
        physics: Res<Time<Physics>>,
        gravity: Res<Gravity>,
    ) {
        if physics.is_paused() || physics.delta().is_zero() {
            return;
        }

        for (parent, target, mut plot) in &mut plots {
            let targets = window_target_entities(target, parent);
            let aggregate =
                aggregate_measures(targets.iter().copied(), &ents, &body_positions, gravity.0);
            let current_time = plot.time;

            for series in &mut plot.series {
                let Some(x) = (series.id.x.measure)(current_time, &aggregate) else {
                    continue;
                };
                let Some(y) = (series.id.y.measure)(current_time, &aggregate) else {
                    continue;
                };
                series.push(current_time, PlotPoint::new(x, y));
            }

            plot.time += physics.delta_secs();
        }
    }

    pub(crate) fn show(
        mut wnds: Query<(
            Entity,
            Option<&ChildOf>,
            Option<&WindowSelectionTarget>,
            &mut InitialPos,
            &mut PlotWindow,
        )>,
        ents: Query<AggregateMeasureData>,
        body_positions: Query<(&Position, &Rotation)>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
        gui_icons: Res<GuiIcons>,
        gravity: Res<Gravity>,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut initial_pos, mut plot) in wnds.iter_mut() {
            let targets = window_target_entities(target, parent);
            if plot.quantities.is_empty() {
                let aggregate =
                    aggregate_measures(targets.iter().copied(), &ents, &body_positions, gravity.0);
                plot.quantities = PLOT_QUANTITIES
                    .iter()
                    .filter_map(|&group| {
                        let measures = group
                            .iter()
                            .filter(|measure| (measure.measure)(plot.time, &aggregate).is_some())
                            .collect::<Vec<_>>();
                        if !measures.is_empty() {
                            Some((group, measures))
                        } else {
                            None
                        }
                    })
                    .collect();
            }
            egui::Window::new("plot").resizable(true).subwindow(
                id,
                ctx,
                &mut initial_pos,
                &mut commands,
                |ui, _commands| {
                    let plot = &mut *plot;
                    let switch_sidebar = egui::Panel::show_switched(
                        ui,
                        &mut plot.sidebar,
                        egui::Panel::left("left_collapsed").resizable(false),
                        egui::Panel::left("left_expanded").resizable(false),
                        |ui, expanded| {
                            let mut switch = false;
                            if expanded {
                                if ui
                                    .add(egui::Button::image(SizedTexture::new(
                                        gui_icons.arrow_left,
                                        [16.0, 16.0],
                                    )))
                                    .clicked()
                                {
                                    switch = true;
                                }
                                if ui
                                    .add(egui::Button::image_and_text(
                                        SizedTexture::new(gui_icons.plot_clear, [16.0, 16.0]),
                                        "Clear",
                                    ))
                                    .clicked()
                                {
                                    for series in &mut plot.series {
                                        series.clear();
                                    }
                                }

                                let quants = plot.quantities.clone();

                                let (x, y, series) = (&mut plot.x, &mut plot.y, &mut plot.series);

                                let mut axis_ =
                                    |name,
                                     this: &mut AxisSetting,
                                     other: &mut AxisSetting,
                                     swap: bool| {
                                        MenuButton::new(format!(
                                            "{}-axis: {}",
                                            name,
                                            this.measures
                                                .iter()
                                                .map(|m| m.name)
                                                .sorted()
                                                .join(", ")
                                        ))
                                        .config(MenuConfig::new().close_behavior(
                                            PopupCloseBehavior::CloseOnClickOutside,
                                        ))
                                        .ui(ui, |ui| {
                                            for (i, (group, measures)) in quants.iter().enumerate()
                                            {
                                                if i > 0 {
                                                    ui.separator();
                                                }
                                                for measure in measures {
                                                    let mut existing =
                                                        this.measures.iter().any(|existing| {
                                                            std::ptr::eq(*existing, *measure)
                                                        });
                                                    if bool_checkbox(
                                                        ui,
                                                        &gui_icons,
                                                        &mut existing,
                                                        measure.name,
                                                    ) {
                                                        if existing {
                                                            if !std::ptr::eq(*group, this.category)
                                                            {
                                                                this.category = group;
                                                                this.measures.clear();
                                                                series.clear();
                                                            }
                                                            for other_measure in
                                                                other.measures.iter()
                                                            {
                                                                let (x_measure, y_measure) = if swap
                                                                {
                                                                    (other_measure, measure)
                                                                } else {
                                                                    (measure, other_measure)
                                                                };
                                                                series.push(PlotSeries::new(
                                                                    x_measure, y_measure,
                                                                ));
                                                            }
                                                            this.measures.push(measure);
                                                        } else {
                                                            series.retain(|series| {
                                                                if swap {
                                                                    !std::ptr::eq(
                                                                        series.id.y,
                                                                        *measure,
                                                                    )
                                                                } else {
                                                                    !std::ptr::eq(
                                                                        series.id.x,
                                                                        *measure,
                                                                    )
                                                                }
                                                            });
                                                            this.measures.retain(|existing| {
                                                                !std::ptr::eq(*existing, *measure)
                                                            });
                                                        }
                                                    }
                                                }
                                            }
                                        });
                                    };

                                axis_("X", x, y, false);
                                axis_("Y", y, x, true);
                                drop(axis_);

                                ui.separator();
                                for (index, series) in plot.series.iter_mut().enumerate() {
                                    bool_checkbox(
                                        ui,
                                        &gui_icons,
                                        &mut series.visible,
                                        egui::RichText::new(&series.id.name)
                                            .strong()
                                            .color(series_color(index)),
                                    );
                                }

                                ui.separator();
                                bool_checkbox(ui, &gui_icons, &mut plot.show_axes, "Show axes");
                                ui.add(
                                    egui::Slider::new(&mut plot.time_span, 1.0..=f32::INFINITY)
                                        .logarithmic(true)
                                        .largest_finite(100.0)
                                        .text("Time span")
                                        .suffix(" s")
                                        .custom(),
                                );
                                let _ = ui.button("Save as CSV file");
                            } else {
                                if ui
                                    .add(egui::Button::image(SizedTexture::new(
                                        gui_icons.arrow_right,
                                        [16.0, 16.0],
                                    )))
                                    .clicked()
                                {
                                    switch = true;
                                }
                                if ui
                                    .add(egui::Button::image(SizedTexture::new(
                                        gui_icons.plot_clear,
                                        [16.0, 16.0],
                                    )))
                                    .clicked()
                                {
                                    for series in &mut plot.series {
                                        series.clear();
                                    }
                                }
                            }
                            switch
                        },
                    )
                    .inner;

                    if switch_sidebar {
                        plot.sidebar = !plot.sidebar;
                    }

                    let current_time = plot.time;
                    let time_span = plot.time_span;
                    let show_axes = plot.show_axes;
                    let series = &plot.series;
                    let fmt = |pos: &HoverPosition| {
                        let HoverPosition::NearDataPoint {
                            plot_name,
                            position,
                            index,
                        } = pos
                        else {
                            return None;
                        };
                        let series = series
                            .iter()
                            .find(|series| series.id.name.as_str() == *plot_name)?;
                        let id = &series.id;
                        let points = series.points_in_time_span(current_time, time_span);
                        let index = *index;
                        let mut label = format!(
                            "x = {:.2} ({})\ny = {:.2} ({})",
                            position.x, id.x, position.y, id.y
                        );

                        if index > 5 {
                            if let Some(previous) = points.get(index - 5) {
                                let delta_x = position.x - previous.x;
                                if delta_x != 0.0 {
                                    label += &format!(
                                        "\ndy/dx = {:.2}",
                                        (position.y - previous.y) / delta_x
                                    );
                                }
                            }
                        }

                        let integral = points
                            .windows(2)
                            .take(index)
                            .map(|values| {
                                (values[0].y + values[1].y) * (values[1].x - values[0].x) / 2.0
                            })
                            .sum::<f64>();
                        label += &format!("\nintegral = {:.2}", integral);
                        Some(label)
                    };

                    egui::CentralPanel::default().show(ui, |ui| {
                        Plot::new("plot")
                            .show_axes(show_axes)
                            .show_grid(show_axes)
                            .label_formatter(fmt)
                            .show(ui, |plot_ui| {
                                for (index, series) in series.iter().enumerate() {
                                    if !series.visible {
                                        continue;
                                    }
                                    plot_ui.line(
                                        Line::new(
                                            series.id.to_string(),
                                            PlotPoints::Borrowed(
                                                series.points_in_time_span(current_time, time_span),
                                            ),
                                        )
                                        .color(series_color(index)),
                                    );
                                }
                            });
                    });
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_span_filters_without_discarding_samples() {
        let mut series = PlotSeries::new(&PLOT_QUANTITIES[0][0], &PLOT_QUANTITIES[2][0]);
        series.push(1.0, PlotPoint::new(1.0, 10.0));
        series.push(5.0, PlotPoint::new(5.0, 50.0));
        series.push(10.0, PlotPoint::new(10.0, 100.0));

        let recent = series.points_in_time_span(10.0, 5.0);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].x, 5.0);
        assert_eq!(series.points.len(), 3);
        assert_eq!(series.points_in_time_span(10.0, f32::INFINITY).len(), 3);
    }

    #[test]
    fn plot_display_defaults_match_the_sidebar_controls() {
        let plot = PlotWindow::default();

        assert!(plot.show_axes);
        assert_eq!(plot.time_span, 30.0);
        assert!(plot.series.iter().all(|series| series.visible));
    }

    #[test]
    fn reinserted_series_moves_to_the_end() {
        let mut series = vec![
            PlotSeries::new(&PLOT_QUANTITIES[0][0], &PLOT_QUANTITIES[2][0]),
            PlotSeries::new(&PLOT_QUANTITIES[0][0], &PLOT_QUANTITIES[2][1]),
        ];

        let first_series = series.remove(0);
        series.push(first_series);

        let names = series
            .iter()
            .map(|series| series.id.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["Velocity (x) / Time", "Speed / Time"]);
    }
}
