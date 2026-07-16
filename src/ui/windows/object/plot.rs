use crate::measures::{GravityData, GravityEnergy, KineticData, KineticEnergy, Momentum};
use crate::objects::spring::SpringObject;
use crate::ui::images::GuiIcons;
use crate::ui::{InitialPos, Subwindow};
use bevy::prelude::ChildOf;
use bevy::prelude::{Commands, Component, Entity, Query, Res, Time, Transform};
use egui_plot::{Line, Plot, PlotPoint, PlotPoints};
use egui::load::SizedTexture;
use bevy_egui::{egui, EguiContexts};
use avian2d::{math::*, prelude::*};
use itertools::Itertools;
use paste::paste;
use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use bevy::ecs::query::{QueryData, WorldQuery};
use crate::egui_systems;

egui_systems!(PlotWindow::show);

#[derive(Component)]
pub struct PlotWindow {
    quantities: Vec<(&'static [PlotQuantity], Vec<&'static PlotQuantity>)>,
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

/*type PlotQuery<'a> = (
    &'a Transform,
    &'a Velocity,
    &'a KineticEnergy,
    &'a GravityEnergy,
    &'a Momentum,
);*/
#[derive(QueryData)]
pub(crate) struct PlotQuery {
    position: Option<&'static Position>,
    lin_velocity: Option<&'static LinearVelocity>,
    ang_velocity: Option<&'static AngularVelocity>,
    kin_data: Option<KineticData>,
    grav_data: Option<GravityData>,
    spring: Option<&'static SpringObject>,
}
type QuantityFn = fn(f32, &PlotQueryItem, &Query<(&Position, &Rotation)>) -> Option<f32>;

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
    if any {
        Some(sum)
    } else {
        None
    }
}

static PLOT_QUANTITIES: &[&[PlotQuantity]] = &[
    &[quantity("Time", |time, _, _| Some(time))],
    &[
        quantity("Position (x)", |_, query, _| Some(query.position?.x)),
        quantity("Position (y)", |_, query, _| Some(query.position?.y)),
    ],
    &[
        quantity("Speed", |_, query, _| Some(query.lin_velocity?.0.length())),
        quantity("Velocity (x)", |_, query, _| Some(query.lin_velocity?.0.x)),
        quantity("Velocity (y)", |_, query, _| Some(query.lin_velocity?.0.y)),
    ],
    &[
        quantity("Angular velocity", |_, query, _| query.ang_velocity.map(|ang| ang.0)),
    ],
    // todo: acceleration
    // todo: force
    &[
        quantity("Momentum (x)", |_, query, _| Some(query.kin_data.as_ref()?.momentum().linear.x)),
        quantity("Momentum (y)", |_, query, _| Some(query.kin_data.as_ref()?.momentum().linear.y)),
    ],
    &[quantity("Angular momentum", |_, query, _| Some(query.kin_data.as_ref()?.momentum().angular))],
    &[
        quantity("Linear kinetic energy", |_, query, _| Some(query.kin_data.as_ref()?.kinetic_energy().linear)),
        quantity("Angular kinetic energy", |_, query, _| Some(query.kin_data.as_ref()?.kinetic_energy().angular)),
        quantity("Kinetic energy (sum)", |_, query, _| Some(query.kin_data.as_ref()?.kinetic_energy().total())),
        quantity("Potential gravitational energy", |_, query, _| Some(query.grav_data.as_ref()?.gravity_energy().energy)),
        quantity("Potential spring energy", |_, query, bodies| query.spring?.potential_energy(bodies)),
        quantity("Potential energy (sum)", |_, query, bodies| {
            let grav = query.grav_data.as_ref().map(|g| g.gravity_energy().energy);
            let spring = query.spring.and_then(|spring| spring.potential_energy(bodies));
            sum_if_any(&[grav, spring])
        }),
        quantity("Energy (sum)", |_, query, bodies| {
            // sum all energies
            // (if no energy *are present* (different from "sum energy is zero"!), return None)
            let kin = query.kin_data.as_ref().map(|k| k.kinetic_energy().total());
            let grav = query.grav_data.as_ref().map(|g| g.gravity_energy().energy);
            let spring = query.spring.and_then(|spring| spring.potential_energy(bodies));
            sum_if_any(&[kin, grav, spring])
        })
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
        mut wnds: Query<(Entity, &ChildOf, &mut InitialPos, &mut PlotWindow)>,
        ents: Query<PlotQuery>,
        body_positions: Query<(&Position, &Rotation)>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
        time: Res<Time>,
        gui_icons: Res<GuiIcons>,
        physics: Res<Time<Physics>>
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, mut initial_pos, mut plot) in wnds.iter_mut() {
            let ent = ents.get(parent.parent()).unwrap();
            if plot.quantities.is_empty() {
                plot.quantities = PLOT_QUANTITIES.iter().filter_map(|&group| {
                    let measures = group.iter().filter(|measure| {
                        (measure.measure)(plot.time, &ent, &body_positions).is_some()
                    }).collect::<Vec<_>>();
                    if !measures.is_empty() {
                        Some((group, measures))
                    } else {
                        None
                    }
                }).collect();
            }

            if !physics.is_paused() {
                let cur_time = plot.time;
                for (name, series) in plot.series.iter_mut() {
                    let Some(x) = (name.x.measure)(cur_time, &ent, &body_positions) else { continue };
                    let Some(y) = (name.y.measure)(cur_time, &ent, &body_positions) else { continue };
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
                        macro_rules! axis {
                            ($name:literal, $sym:ident, $other:ident) => {
                                paste! {
                                    ui.menu_button(format!("{}-axis: {}", $name, plot.[<measures_ $sym>].iter().map(|m| m.name).sorted().join(", ")), |ui| {
                                        for (i, (group, measures)) in quants.iter().enumerate() {
                                            if i > 0 {
                                                ui.separator();
                                            }
                                            for [<$sym _measure>] in measures {
                                                let mut existing = plot.[<measures_ $sym>].contains([<$sym _measure>]);
                                                if ui.checkbox(&mut existing, [<$sym _measure>].name).changed() {
                                                    if existing {
                                                        if !std::ptr::eq(*group, plot.[<category_ $sym>]) {
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
                                                        plot.[<measures_ $sym>].remove([<$sym _measure>]);
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
                                plot_ui.line(Line::new(format!("{name:?}"), PlotPoints::Owned(series.values.clone())));
                            }
                        });
                });
        }
    }
}
