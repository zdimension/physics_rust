use crate::measures::{GravityEnergy, KineticEnergy, Momentum};
use crate::ui::images::GuiIcons;
use crate::ui::{BevyIdThing, InitialPos, Subwindow};
use bevy::hierarchy::Parent;
use bevy::prelude::{Commands, Component, Entity, Query, Res, Time, Transform};
use bevy_egui::egui::plot::{Line, Plot, PlotPoint, PlotPoints};
use bevy_egui::{egui, EguiContexts};
use bevy_rapier2d::dynamics::Velocity;
use bevy_rapier2d::plugin::RapierConfiguration;
use itertools::Itertools;
use paste::paste;
use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use crate::systems;

systems!(PlotWindow::show);

#[derive(Component)]
pub struct PlotWindow {
    series: HashMap<PlotSeriesId, PlotSeries>,
    category_x: &'static [PlotQuantity],
    measures_x: HashSet<&'static PlotQuantity>,
    category_y: &'static [PlotQuantity],
    measures_y: HashSet<&'static PlotQuantity>,
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

type PlotQuery<'a> = (
    &'a Transform,
    &'a Velocity,
    &'a KineticEnergy,
    &'a GravityEnergy,
    &'a Momentum,
);
type QuantityFn = fn(f32, PlotQuery) -> f32;

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

static PLOT_QUANTITIES: &[&[PlotQuantity]] = &[
    &[quantity("Time", |time, _| time)],
    &[
        quantity("Position (x)", |_, query| query.0.translation.x),
        quantity("Position (y)", |_, query| query.0.translation.y),
    ],
    &[
        quantity("Speed", |_, query| query.1.linvel.length()),
        quantity("Velocity (x)", |_, query| query.1.linvel.x),
        quantity("Velocity (y)", |_, query| query.1.linvel.y),
    ],
    &[quantity("Angular velocity", |_, query| query.1.angvel)],
    // todo: acceleration
    // todo: force
    &[
        quantity("Momentum (x)", |_, query| query.4.linear.x),
        quantity("Momentum (y)", |_, query| query.4.linear.y),
    ],
    &[quantity("Angular momentum", |_, query| query.4.angular)],
    &[
        quantity("Linear kinetic energy", |_, query| query.2.linear),
        quantity("Angular kinetic energy", |_, query| query.2.angular),
        quantity("Kinetic energy (sum)", |_, query| query.2.total()),
        quantity("Potential gravitational energy", |_, query| query.3.energy),
        quantity("Potential energy (sum)", |_, query| query.3.energy),
        quantity("Energy (sum)", |_, query| query.2.total() + query.3.energy),
    ],
];

impl Default for PlotWindow {
    fn default() -> Self {
        Self {
            series: HashMap::from([(
                PlotSeriesId::new(&PLOT_QUANTITIES[0][0], &PLOT_QUANTITIES[2][0]),
                PlotSeries::new(),
            )]),
            category_x: PLOT_QUANTITIES[0],
            measures_x: HashSet::from([&PLOT_QUANTITIES[0][0]]),
            category_y: PLOT_QUANTITIES[2],
            measures_y: HashSet::from([&PLOT_QUANTITIES[2][0]]),
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
        mut wnds: Query<(Entity, &Parent, &mut InitialPos, &mut PlotWindow)>,
        ents: Query<PlotQuery>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
        rapier_conf: Res<RapierConfiguration>,
        time: Res<Time>,
        gui_icons: Res<GuiIcons>,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, parent, mut initial_pos, mut plot) in wnds.iter_mut() {
            if rapier_conf.physics_pipeline_active {
                let data = ents.get(parent.get()).unwrap();
                let cur_time = plot.time;
                for (name, series) in plot.series.iter_mut() {
                    let x = (name.x.measure)(cur_time, data);
                    let y = (name.y.measure)(cur_time, data);
                    series.values.push(PlotPoint::new(x, y));
                }
                plot.time += time.delta_seconds();
            }
            egui::Window::new("plot")
                .resizable(true)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    let series = unsafe { &*(&plot.series as *const HashMap<PlotSeriesId, PlotSeries>) };
                    let fmt = |name: &str, value: &PlotPoint| {
                        if name.len() > 0 {
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
                        if ui.add(egui::Button::image_and_text(gui_icons.plot_clear, [16.0, 16.0], "Clear"))
                            .clicked() {
                            for series in plot.series.values_mut() {
                                series.values.clear();
                            }
                        }

                        macro_rules! axis {
                            ($name:literal, $sym:ident, $other:ident) => {
                                paste! {
                                    ui.menu_button(format!("{}-axis: {}", $name, plot.[<measures_ $sym>].iter().map(|m| m.name).sorted().join(", ")), |ui| {
                                        for (i, &group) in PLOT_QUANTITIES.iter().enumerate() {
                                            if i > 0 {
                                                ui.separator();
                                            }
                                            for [<$sym _measure>] in group {
                                                let mut existing = plot.[<measures_ $sym>].contains(&[<$sym _measure>]);
                                                if ui.checkbox(&mut existing, [<$sym _measure>].name).changed() {
                                                    if existing {
                                                        if !std::ptr::eq(group, plot.[<category_ $sym>]) {
                                                            plot.[<category_ $sym>] = group;
                                                            plot.[<measures_ $sym>].clear();
                                                            plot.series.clear();
                                                        }
                                                        let plot = &mut *plot;
                                                        for [<$other _measure>] in plot.[<measures_ $other>].iter() {
                                                            plot.series.insert(PlotSeriesId::new(x_measure, y_measure), PlotSeries::new());
                                                        }
                                                        plot.[<measures_ $sym>].insert([<$sym _measure>]);
                                                    } else {
                                                        plot.series.retain(|id, _| id.$sym != [<$sym _measure>]);
                                                        plot.[<measures_ $sym>].remove(&[<$sym _measure>]);
                                                    }
                                                }
                                            }
                                        }
                                    });
                                }
                            }
                        }

                        axis!("X", x, y);
                        axis!("Y", y, x);
                    });
                    Plot::new("plot")
                        .label_formatter(fmt)
                        .show(ui, |plot_ui| {
                            for (name, series) in &plot.series {
                                plot_ui.line(Line::new(PlotPoints::Owned(series.values.clone())).name(name));
                            }
                        });
                });
        }
    }
}
