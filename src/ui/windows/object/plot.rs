use crate::egui_systems;
use crate::measures::{AggregateMeasureData, AggregateMeasures, aggregate_measures};
use crate::ui::images::GuiIcons;
use crate::ui::{
    InitialPos, Subwindow, WindowSelectionTarget, bool_checkbox, window_target_entities,
};
use avian2d::prelude::*;
use bevy::prelude::ChildOf;
use bevy::prelude::{Commands, Component, Entity, Query, Res, Time};
use bevy_egui::{EguiContexts, egui};
use egui::containers::PopupCloseBehavior;
use egui::containers::menu::{MenuButton, MenuConfig};
use egui::load::SizedTexture;
use egui_plot::{Line, Plot, PlotPoint, PlotPoints};
use itertools::Itertools;
use paste::paste;
use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};

egui_systems!(PlotWindow::show);

struct AxisSetting {
    category: &'static [PlotQuantity],
    measures: HashSet<&'static PlotQuantity>,
}

#[derive(Component)]
pub struct PlotWindow {
    quantities: Vec<(&'static [PlotQuantity], Vec<&'static PlotQuantity>)>,
    series: HashMap<PlotSeriesId, PlotSeries>,
    /*category_x: &'static [PlotQuantity],
    measures_x: HashSet<&'static PlotQuantity>,
    category_y: &'static [PlotQuantity],
    measures_y: HashSet<&'static PlotQuantity>,*/
    x: AxisSetting,
    y: AxisSetting,
    time: f32,
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

impl Hash for PlotSeriesId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl PartialEq for PlotSeriesId {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.x, other.x) && std::ptr::eq(self.y, other.y)
    }
}

impl Eq for PlotSeriesId {}

impl Display for PlotSeriesId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl Borrow<str> for PlotSeriesId {
    fn borrow(&self) -> &str {
        &self.name
    }
}

impl Debug for PlotSeriesId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

struct PlotSeries {
    values: Vec<PlotPoint>,
}

impl PlotSeries {
    fn new() -> Self {
        Self { values: Vec::new() }
    }
}

/*type PlotQuery<'a> = (
    &'a Transform,
    &'a Velocity,
    &'a KineticEnergy,
    &'a GravityEnergy,
    &'a Momentum,
);*/
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

type PlotQuantityCategory = &'static [PlotQuantity];

const fn quantity(name: &'static str, measure: QuantityFn) -> PlotQuantity {
    PlotQuantity { name, measure }
}

fn sum_if_any(items: &[Option<f32>]) -> Option<f32> {
    let mut sum = 0.0;
    let mut any = false;
    for item in items {
        if let Some(value) = item {
            sum += value;
            any = true;
        }
    }
    if any { Some(sum) } else { None }
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

/*static PLOT_QUANTITIES_2: () = &[
    &[("Time", |time, _| time)],
    |query| query.transform
]*/

impl Default for PlotWindow {
    fn default() -> Self {
        Self {
            quantities: vec![],
            series: HashMap::from([(
                PlotSeriesId::new(&PLOT_QUANTITIES[0][0], &PLOT_QUANTITIES[2][0]),
                PlotSeries::new(),
            )]),
            x: AxisSetting {
                category: PLOT_QUANTITIES[0],
                measures: HashSet::from([&PLOT_QUANTITIES[0][0]]),
            },
            y: AxisSetting {
                category: PLOT_QUANTITIES[2],
                measures: HashSet::from([&PLOT_QUANTITIES[2][0]]),
            },
            time: 0.0,
        }
    }
}

impl Hash for &'static PlotQuantity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (*self as *const PlotQuantity).hash(state);
    }
}

impl PartialEq for &'static PlotQuantity {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(*self, *other)
    }
}

impl Eq for &'static PlotQuantity {}

impl PlotWindow {
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
        time: Res<Time>,
        gui_icons: Res<GuiIcons>,
        physics: Res<Time<Physics>>,
        gravity: Res<Gravity>,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut initial_pos, mut plot) in wnds.iter_mut() {
            let targets = window_target_entities(target, parent);
            let aggregate =
                aggregate_measures(targets.iter().copied(), &ents, &body_positions, gravity.0);
            if plot.quantities.is_empty() {
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

            if !physics.is_paused() {
                let cur_time = plot.time;
                let aggregate =
                    aggregate_measures(targets.iter().copied(), &ents, &body_positions, gravity.0);
                for (name, series) in plot.series.iter_mut() {
                    let Some(x) = (name.x.measure)(cur_time, &aggregate) else {
                        continue;
                    };
                    let Some(y) = (name.y.measure)(cur_time, &aggregate) else {
                        continue;
                    };
                    series.values.push(PlotPoint::new(x, y));
                }
                plot.time += time.delta_secs();
            }
            egui::Window::new("plot")
                .resizable(true)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    let series = unsafe { &*(&plot.series as *const HashMap<PlotSeriesId, PlotSeries>) };
                    let fmt = |name: &str, value: &PlotPoint| {
                        if !name.is_empty() {
                            let (id, series) = series.get_key_value(name).unwrap_or_else(|| panic!("series {} not found, available: {:?}", name, series.keys()));
                            let mut base = format!("x = {:.2} ({})\ny = {:.2} ({})", value.x, id.x, value.y, id.y);
                            let values = &series.values;
                            let idx = values.binary_search_by(|probe| probe.x.total_cmp(&value.x));
                            if let Ok(idx) = idx {
                                if idx > 5 {
                                    let prev = &values[idx - 5];
                                    let slope = (value.y - prev.y) / (value.x - prev.x);
                                    base += &format!("\ndy/dx = {:.2}", slope);
                                }

                                let integ = values.windows(2).take(idx).map(|w| (w[0].y + w[1].y) * (w[1].x - w[0].x) / 2.0).sum::<f64>();
                                base += &format!("\n∫dt = {:.2}", integ);
                            }
                            base
                        } else {
                            String::from("")
                        }
                    };
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::image_and_text(SizedTexture::new(gui_icons.plot_clear, [16.0, 16.0]), "Clear"))
                            .clicked() {
                            for series in plot.series.values_mut() {
                                series.values.clear();
                            }
                        }

                        let quants = plot.quantities.clone();

                        let plot = &mut *plot;
                        let (x, y, series) = (&mut plot.x, &mut plot.y, &mut plot.series);

                        let mut axis_ = |name, this: &mut AxisSetting, other: &mut AxisSetting, swap: bool| {
                            MenuButton::new(format!("{}-axis: {}", name, this.measures.iter().map(|m| m.name).sorted().join(", ")))
                                .config(MenuConfig::new().close_behavior(PopupCloseBehavior::CloseOnClickOutside))
                                .ui(ui, |ui| {
                                    for (i, (group, measures)) in quants.iter().enumerate() {
                                        if i > 0 {
                                            ui.separator();
                                        }
                                        for measure in measures {
                                            let mut existing = this.measures.contains(measure);
                                            if bool_checkbox(ui, &gui_icons, &mut existing, measure.name) {
                                                if existing {
                                                    if !std::ptr::eq(*group, this.category) {
                                                        this.category = group;
                                                        this.measures.clear();
                                                        series.clear();
                                                    }
                                                    for other_measure in other.measures.iter() {
                                                        let (x_measure, y_measure) = if swap {
                                                            (other_measure, measure)
                                                        } else {
                                                            (measure, other_measure)
                                                        };
                                                        series.insert(PlotSeriesId::new(x_measure, y_measure), PlotSeries::new());
                                                    }
                                                    this.measures.insert(measure);
                                                } else {
                                                    series.retain(|id, _| {
                                                        if swap {
                                                            id.y != measure
                                                        } else {
                                                            id.x != measure
                                                        }
                                                    });
                                                    this.measures.remove(measure);
                                                }
                                            }
                                        }
                                    }
                                });
                        };

                        axis_("X", x, y, false);
                        axis_("Y", y, x, true);
                    });
                    Plot::new("plot")
                        .label_formatter(fmt)
                        .show(ui, |plot_ui| {
                            for (name, series) in &plot.series {
                                plot_ui.line(Line::new(format!("{name:?}"), PlotPoints::Owned(series.values.clone())));
                            }
                        });
                });
        }
    }
}
