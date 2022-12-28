use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use crate::{GuiIcons, ToolIcons, UiState};
use std::time::Duration;
use std::fmt::Display;
use bevy::log::info;
use bevy::math::{Vec2, Vec3};
use bevy::prelude::*;
use bevy_egui::egui::{Align2, Context, Id, InnerResponse, NumExt, pos2, Pos2, Separator, Ui, vec2, Widget};
use bevy_egui::{egui, EguiContext};
use bevy_egui::egui::plot::{Line, Plot, PlotPoint, PlotPoints};
use bevy_mouse_tracking_plugin::MainCamera;
use bevy_rapier2d::plugin::{RapierConfiguration, TimestepMode};
use bevy_rapier2d::prelude::*;
use derivative::Derivative;
use lyon_path::commands::CommandsPathSlice;
use icon_button::IconButton;
use separator_custom::SeparatorCustom;
use paste::paste;
use itertools::Itertools;
mod icon_button;
mod menu_item;
mod separator_custom;

use menu_item::MenuItem;
use crate::measures::{GravityEnergy, KineticEnergy, Momentum};

#[derive(Derivative)]
#[derivative(Debug)]
pub struct WindowData {
    entity: Option<Entity>,
    #[derivative(Debug = "ignore")]
    handler: Box<dyn FnMut(&Context, &Time, &mut UiState, &mut Commands) + Sync + Send>,
}

impl WindowData {
    fn new(
        entity: Option<Entity>,
        handler: impl FnMut(&Context, &Time, &mut UiState, &mut Commands) + Sync + Send + 'static,
    ) -> Self {
        Self {
            entity,
            handler: Box::new(handler),
        }
    }
}

pub struct GravitySetting {
    value: Vec2,
    enabled: bool,
}

impl Default for GravitySetting {
    fn default() -> Self {
        Self {
            value: Vec2::new(0.0, -9.81),
            enabled: true,
        }
    }
}

pub fn ui_example(
    mut egui_ctx: ResMut<EguiContext>,
    ui_state: ResMut<UiState>,
    mut is_initialized: Local<bool>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
) {
    if !*is_initialized {
        let mut camera = cameras.single_mut();
        camera.scale = Vec3::new(0.01, 0.01, 1.0);
        *is_initialized = true;
    }

    egui::TopBottomPanel::top("top_panel").show(egui_ctx.ctx_mut(), |ui| {
        // The top panel is often a good place for a menu bar:
        egui::menu::bar(ui, |ui| {
            egui::menu::menu_button(ui, "File", |ui| {
                if ui.button("Quit").clicked() {
                    std::process::exit(0);
                }
            });
        });
    });

    egui::Window::new("Debug").show(egui_ctx.clone().ctx_mut(), |ui| {
        ui.monospace(format!("{:#?}", ui_state));
    });
}

pub fn draw_ui() -> SystemSet {
    SystemSet::new()
        .with_system(ui_example)
        .with_system(draw_toolbox)
        .with_system(draw_bottom_toolbar)
        .with_system(process_temporary_windows)
        .with_system(remove_temporary_windows)
        .with_system(MenuWindow::show)
        .with_system(InformationWindow::show)
        .with_system(PlotWindow::show)
}

pub fn draw_toolbox(
    mut egui_ctx: ResMut<EguiContext>,
    mut ui_state: ResMut<UiState>,
    tool_icons: Res<ToolIcons>,
    mut clear_tmp: EventWriter<RemoveTemporaryWindowsEvent>
) {
    egui::Window::new("Tools")
        .anchor(Align2::LEFT_BOTTOM, [0.0, 0.0])
        .title_bar(false)
        .resizable(false)
        .default_size(egui::Vec2::ZERO)
        .show(egui_ctx.clone().ctx_mut(), |ui| {
            ui.vertical(|ui| {
                let ui_state = &mut *ui_state;
                for (i, category) in ui_state.toolbox.iter().enumerate() {
                    if i > 0 {
                        ui.add(SeparatorCustom::default().horizontal());
                    }
                    for chunk in category.chunks(2) {
                        ui.horizontal(|ui| {
                            for def in chunk {
                                if ui
                                    .add(
                                        IconButton::new(
                                            egui_ctx.add_image(def.icon(&tool_icons)),
                                            24.0,
                                        )
                                        .selected(ui_state.toolbox_selected.is_same(def)),
                                    )
                                    .clicked()
                                {
                                    ui_state.toolbox_selected = *def;
                                    clear_tmp.send(RemoveTemporaryWindowsEvent);
                                }
                            }
                        });
                    }
                }
            });
        });
}

pub fn draw_bottom_toolbar(
    mut egui_ctx: ResMut<EguiContext>,
    mut ui_state: ResMut<UiState>,
    mut rapier: ResMut<RapierConfiguration>,
    mut gravity_conf: Local<GravitySetting>,
    tool_icons: Res<ToolIcons>,
    gui_icons: Res<GuiIcons>,
    mut clear_tmp: EventWriter<RemoveTemporaryWindowsEvent>
) {
    egui::Window::new("Tools2")
        .anchor(Align2::CENTER_BOTTOM, [0.0, 0.0])
        .title_bar(false)
        .resizable(false)
        .show(egui_ctx.clone().ctx_mut(), |ui| {
            ui.horizontal(|ui| {
                let ui_state = &mut *ui_state;
                for def in ui_state.toolbox_bottom.iter() {
                    if ui
                        .add(
                            IconButton::new(egui_ctx.add_image(def.icon(&tool_icons)), 32.0)
                                .selected(ui_state.toolbox_selected.is_same(def)),
                        )
                        .clicked()
                    {
                        ui_state.toolbox_selected = *def;
                        clear_tmp.send(RemoveTemporaryWindowsEvent);
                    }
                }

                ui.separator();

                let playpause = ui.add(IconButton::new(
                    if rapier.physics_pipeline_active {
                        gui_icons.pause
                    } else {
                        gui_icons.play
                    },
                    32.0,
                ));

                if playpause.clicked() {
                    rapier.physics_pipeline_active = !rapier.physics_pipeline_active;
                }
                playpause.context_menu(|ui| {
                    let (max_dt, mut time_scale, substeps) = match rapier.timestep_mode {
                        TimestepMode::Variable {
                            max_dt,
                            time_scale,
                            substeps,
                        } => (max_dt, time_scale, substeps),
                        _ => unreachable!("Shouldn't happen"),
                    };
                    ui.add(
                        egui::Slider::new(&mut time_scale, 0.1..=10.0)
                            .logarithmic(true)
                            .text("Simulation speed"),
                    );
                    rapier.timestep_mode = TimestepMode::Variable {
                        max_dt,
                        time_scale,
                        substeps,
                    };
                });

                ui.separator();

                let gravity =
                    ui.add(IconButton::new(gui_icons.gravity, 32.0).selected(gravity_conf.enabled));
                if gravity.clicked() {
                    gravity_conf.enabled = !gravity_conf.enabled;
                    if gravity_conf.enabled {
                        rapier.gravity = gravity_conf.value;
                    } else {
                        rapier.gravity = Vec2::ZERO;
                    }
                }
            })
        });
}

trait AsPos2 {
    fn as_pos2(&self) -> egui::Pos2;
}

impl AsPos2 for Vec2 {
    fn as_pos2(&self) -> egui::Pos2 {
        pos2(self.x, self.y)
    }
}

impl AsPos2 for Pos2 {
    fn as_pos2(&self) -> egui::Pos2 {
        *self
    }
}

#[derive(Component)]
struct InitialPos(Pos2, Pos2);

impl InitialPos {
    fn initial(pos: impl AsPos2) -> impl Bundle {
        (Self::persistent(pos), TemporaryWindow)
    }

    fn persistent(pos: impl AsPos2) -> InitialPos {
        let pos = pos.as_pos2();
        Self(pos, pos)
    }

    fn update<T>(&mut self, resp: InnerResponse<T>) {
        self.1 = resp.response.rect.left_top();
    }
}

#[derive(Component)]
pub struct TemporaryWindow;

pub struct ContextMenuEvent {
    pub screen_pos: Vec2,
}

pub fn handle_context_menu(
    mut ev: EventReader<ContextMenuEvent>,
    mut ui: ResMut<UiState>,
    mut commands: Commands,
) {
    for ev in ev.iter() {
        let entity = ui.selected_entity.map(|sel| sel.entity);
        info!("context menu at {:?} for {:?}", ev.screen_pos, entity);
        let wnd = commands
            .spawn((MenuWindow::default(), InitialPos::initial(ev.screen_pos)))
            .id();

        if let Some(id) = entity {
            commands.entity(id).push_children(&[wnd]);
        }
    }
}

fn process_temporary_windows(
    wnds: Query<(Entity, &InitialPos, &TemporaryWindow)>,
    mut commands: Commands,
) {
    for (wnd, pos, _) in wnds.iter() {
        if pos.0 != pos.1 {
            commands.entity(wnd).remove::<TemporaryWindow>();
        }
    }
}

/*
#[derive(Component)]
struct EntityWindow {
    entity: Option<Entity>
}
*/
trait BevyIdThing {
    fn id_bevy(self, id: Entity) -> Self;
}

impl<'a> BevyIdThing for egui::Window<'a> {
    fn id_bevy(mut self, id: Entity) -> Self {
        self.id(Id::new(id))
    }
}

#[derive(Default, Component)]
struct MenuWindow {
    hovered_item: Option<(MenuId, Duration)>,
    selected_item: Option<MenuId>,
    our_child_window: Option<Entity>,
}

impl Into<Pos2> for &InitialPos {
    fn into(self) -> Pos2 {
        self.0
    }
}

trait Subwindow {
    fn subwindow(self, id: Entity, ctx: &Context, initial_pos: &mut InitialPos, commands: &mut Commands, contents: impl FnOnce(&mut Ui, &mut Commands));
}

impl<'a> Subwindow for egui::Window<'a> {
    fn subwindow(mut self, id: Entity, ctx: &Context, initial_pos: &mut InitialPos, commands: &mut Commands, contents: impl FnOnce(&mut Ui, &mut Commands)) {
        let mut open = true;
        self
            .id_bevy(id)
            .default_pos(&*initial_pos)
            .open(&mut open)
            .show(ctx, |ui| {
                contents(ui, commands)
            })
            .map(|resp| initial_pos.1 = resp.response.rect.left_top());
        if !open {
            commands.entity(id).despawn_recursive();
        }
    }
}

impl MenuWindow {
    fn show(
        mut wnds: Query<(Entity, Option<&Parent>, &mut MenuWindow, &mut InitialPos)>,
        time: Res<Time>,
        mut egui_ctx: ResMut<EguiContext>,
        icons: Res<GuiIcons>,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, entity, mut info_wnd, mut initial_pos) in wnds.iter_mut() {
            let entity = entity.map(Parent::get);
            egui::Window::new("context menu")
                .default_size(vec2(0.0, 0.0))
                .resizable(false)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    macro_rules! item {
                            (@ $text:literal, $icon:expr) => {
                                ui.add(MenuItem::button($icon, $text.to_string())).clicked()
                            };
                            ($text:literal, $icon:ident) => {
                                item!(@ $text, Some(icons.$icon))
                            };
                            ($text:literal) => {
                                item!(@ $text, None)
                            };
                        }
                    macro_rules! menu {
                            (@ $text: literal, $icon: expr, $wnd:ty) => {
                                let our_id = $text;
                                let us_selected = matches!(info_wnd.selected_item, Some(id) if id == our_id);
                                let menu = ui.add(MenuItem::menu($icon, $text.to_string(), icons.arrow_right).selected(us_selected));

                                if !us_selected {
                                    let selected = match info_wnd.hovered_item {
                                        Some((id, at)) if id == our_id && (time.elapsed() - at) > Duration::from_millis(500) => true,
                                        _ => menu.clicked()
                                    };

                                    if selected {
                                        info!("clicked: {}", $text);
                                        info_wnd.selected_item = Some(our_id);

                                        if let Some(id) = info_wnd.our_child_window {
                                            commands.entity(id).despawn_recursive();
                                        }

                                        info!("rect: {:?}", menu.rect);

                                        let new_wnd = commands.spawn((
                                            <$wnd as Default>::default(),
                                            InitialPos::initial(menu.rect.right_top())
                                        )).id();

                                        if let Some(id) = entity {
                                            commands.entity(id).push_children(&[new_wnd]);
                                        }

                                        info_wnd.our_child_window = Some(new_wnd);
                                    }
                                }

                                let us = matches!(info_wnd.hovered_item, Some((id, _)) if id == our_id);
                                if menu.hovered() && !us { // we're hovering but someone else was
                                    info_wnd.hovered_item = Some((our_id, time.elapsed())); // we're the new hoverer
                                } else if !menu.hovered() && us { // not hovering and we were
                                    info_wnd.hovered_item = None; // now we're not
                                }
                            };
                            ($text:literal, $icon:ident, $wnd:ty) => {
                                menu!(@ $text, Some(icons.$icon), $wnd);
                            };
                            ($text:literal, /, $wnd:ty) => {
                                menu!(@ $text, None, $wnd);
                            };
                        }

                    match entity {
                        Some(id) => {
                            if item!("Erase", erase) {
                                commands.entity(id).despawn_recursive();
                            }
                            if item!("Mirror", mirror) {}
                            if item!("Show plot", plot) {
                                commands.entity(id).with_children(|parent| {
                                    parent.spawn((PlotWindow::default(), InitialPos::persistent(pos2(100.0, 100.0))));
                                });
                            }
                            ui.add(Separator::default().horizontal());

                            menu!("Selection", /, SelectionWindow);
                            menu!("Appearance", color, AppearanceWindow);
                            menu!("Text", text, TextWindow);
                            menu!("Material", material, MaterialWindow);
                            menu!("Velocities", velocity, VelocitiesWindow);
                            menu!("Information", info, InformationWindow);
                            menu!("Collision layers", collisions, CollisionsWindow);
                            menu!("Geometry actions", /, GeometryActionsWindow);
                            menu!("Combine shapes", csg, CombineShapesWindow);
                            menu!("Controller", controller, ControllerWindow);
                            menu!("Script menu", /, ScriptMenuWindow);
                        }
                        None => {
                            if item!("Zoom to scene", zoom2scene) {}
                            if item!("Default view") {}
                            if item!("Background", color) {}
                        }
                    }
                });
        }
    }
}

type MenuId = &'static str;

#[derive(Default, Component)]
struct SelectionWindow;

#[derive(Default, Component)]
struct AppearanceWindow;

#[derive(Default, Component)]
struct TextWindow;

#[derive(Default, Component)]
struct MaterialWindow;

#[derive(Default, Component)]
struct VelocitiesWindow;

#[derive(Default, Component)]
struct InformationWindow;

impl InformationWindow {
    fn show(
        mut wnds: Query<(Entity, &Parent, &mut InitialPos), With<InformationWindow>>,
        ents: Query<(
            Option<&Transform>,
            Option<&ReadMassProperties>,
            Option<&Velocity>,
            Option<&ColliderMassProperties>,
            Option<&KineticEnergy>
        )>,
        rapier_conf: Res<RapierConfiguration>,
        mut egui_ctx: ResMut<EguiContext>,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let (
                xform,
                mass,
                vel,
                coll_mass,
                kine
            ) = ents.get(parent.get()).unwrap();
            egui::Window::new("info")
                .id_bevy(id)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    fn line(ui: &mut Ui, label: &'static str, val: String) {
                        ui.label(label);
                        ui.label(val);
                        ui.end_row();
                    }
                    egui::Grid::new("info grid").striped(true).show(ui, |ui| {
                        if let Some(ReadMassProperties(mass)) = mass {
                            line(ui, "Mass", format!("{:.3} kg", mass.mass));

                            line(ui, "Moment of inertia", format!("{:.3} kgm²", mass.principal_inertia));
                        }

                        if let Some(props) = coll_mass {
                            line(ui, "Collider mass", format!("{:?}", props));
                        }

                        if let Some(xform) = xform {
                            line(ui, "Position", format!("[x={:.3}, y={:.3}] m", xform.translation.x, xform.translation.y));
                        }

                        if let Some(vel) = vel {
                            line(ui, "Velocity", format!("[x={:.3}, y={:.3}] m/s", vel.linvel.x, vel.linvel.y));

                            line(ui, "Angular velocity", format!("{:.3} rad/s", vel.angvel));
                        }
                    });
                    ui.separator();
                    egui::Grid::new("info grid 2").striped(true).show(ui, |ui| {
                        let mut total = 0.0;

                        if let Some(KineticEnergy { linear, angular }) = kine {
                            line(ui, "Kinetic linear energy", format!("{:.3} J", linear));
                            line(ui, "Kinetic angular energy", format!("{:.3} J", angular));
                            total += linear + angular;
                        }

                        if let Some(ReadMassProperties(mass)) = mass {
                            let pot = mass.mass * -rapier_conf.gravity.y * xform.unwrap().translation.y;
                            line(ui, "Potential energy (gravity)", format!("{:.3} J", pot)); // todo: nonvertical gravity
                            total += pot;
                        }

                        line(ui, "Energy (total)", format!("{:.3} J", total));
                    });
                });
        }
    }
}

#[derive(Default, Component)]
struct CollisionsWindow;

#[derive(Default, Component)]
struct GeometryActionsWindow;

#[derive(Default, Component)]
struct CombineShapesWindow;

#[derive(Default, Component)]
struct ControllerWindow;

#[derive(Default, Component)]
struct ScriptMenuWindow;


pub struct RemoveTemporaryWindowsEvent;

fn remove_temporary_windows(
    mut commands: Commands,
    mut events: EventReader<RemoveTemporaryWindowsEvent>,
    wnds: Query<Entity, With<TemporaryWindow>>,
) {
    for _ in events.iter() {
        for id in wnds.iter() {
            commands.entity(id).despawn_recursive();
        }
    }
}

#[derive(Component)]
struct PlotWindow {
    series: HashMap<PlotSeriesId, PlotSeries>,
    category_x: &'static [PlotQuantity],
    measures_x: HashSet<&'static PlotQuantity>,
    category_y: &'static [PlotQuantity],
    measures_y: HashSet<&'static PlotQuantity>,
    time: f32
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
            y
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
    values: Vec<PlotPoint>
}

impl PlotSeries {
    fn new() -> Self {
        Self { values: Vec::new() }
    }
}

type PlotQuery<'a> = (&'a Transform, &'a Velocity, &'a KineticEnergy, &'a GravityEnergy, &'a Momentum);
type QuantityFn = fn(f32, PlotQuery) -> f32;

struct PlotQuantity {
    name: &'static str,
    measure: QuantityFn
}

impl Display for PlotQuantity {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

type PlotQuantityCategory = &'static [PlotQuantity];

fn quantity(name: &'static str, measure: QuantityFn) -> PlotQuantity {
    PlotQuantity { name, measure }
}

static PLOT_QUANTITIES: &[&[PlotQuantity]] = &[
    &[
        PlotQuantity { name: "Time", measure: |time, _| time },
    ],
    &[
        PlotQuantity { name: "Position (x)", measure: |_, query| query.0.translation.x },
        PlotQuantity { name: "Position (y)", measure: |_, query| query.0.translation.y },
    ],
    &[
        PlotQuantity { name: "Speed", measure: |_, query| query.1.linvel.length() },
        PlotQuantity { name: "Velocity (x)", measure: |_, query| query.1.linvel.x },
        PlotQuantity { name: "Velocity (y)", measure: |_, query| query.1.linvel.y },
    ],
    &[
        PlotQuantity { name: "Angular velocity", measure: |_, query| query.1.angvel },
    ],
    // todo: acceleration
    // todo: force
    &[
        PlotQuantity { name: "Momentum (x)", measure: |_, query| query.4.linear.x },
        PlotQuantity { name: "Momentum (y)", measure: |_, query| query.4.linear.y },
    ],
    &[
        PlotQuantity { name: "Angular momentum", measure: |_, query| query.4.angular },
    ],
    &[
        PlotQuantity { name: "Linear kinetic energy", measure: |_, query| query.2.linear },
        PlotQuantity { name: "Angular kinetic energy", measure: |_, query| query.2.angular },
        PlotQuantity { name: "Kinetic energy (sum)", measure: |_, query| query.2.total() },
        PlotQuantity { name: "Potential gravitational energy", measure: |_, query| query.3.energy },
        PlotQuantity { name: "Potential energy (sum)", measure: |_, query| query.3.energy },
        PlotQuantity { name: "Energy (sum)", measure: |_, query| query.2.total() + query.3.energy },
    ],
];

impl Default for PlotWindow {
    fn default() -> Self {
        Self {
            series: HashMap::from([(PlotSeriesId::new(&PLOT_QUANTITIES[0][0], &PLOT_QUANTITIES[2][0]), PlotSeries::new())]),
            category_x: PLOT_QUANTITIES[0],
            measures_x: HashSet::from([&PLOT_QUANTITIES[0][0]]),
            category_y: PLOT_QUANTITIES[2],
            measures_y: HashSet::from([&PLOT_QUANTITIES[2][0]]),
            time: 0.0
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
    /*fn make_series(x: &'static PlotQuantity, y: &'static PlotQuantity) -> (PlotSeriesId, PlotSeries) {
        (PlotSeriesId::new(x, y), PlotSeries::new())
    }

    fn add_series(&mut self, x_cat: PlotQuantityCategory, x: &'static PlotQuantity, y_cat: PlotQuantityCategory, y: &'static PlotQuantity) {
        if !std::ptr::eq(x_cat, self.category_x) || !std::ptr::eq(y_cat, self.category_y) {
            self.category_x = x_cat;
            self.category_y = y_cat;
            self.measures_x.clear();
            self.measures_y.clear();
            self.series.clear();
        }
        self.series.insert(PlotSeriesId::new(x, y), PlotSeries::new());
        self.measures_x.insert(x);
        self.measures_y.insert(y);
    }*/

    fn show(
        mut wnds: Query<(Entity, &Parent, &mut InitialPos, &mut PlotWindow)>,
        ents: Query<PlotQuery>,
        mut egui_ctx: ResMut<EguiContext>,
        mut commands: Commands,
        rapier_conf: Res<RapierConfiguration>,
        time: Res<Time>,
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
                .id_bevy(id)
                .resizable(true)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    let series = unsafe { &*(&plot.series as *const HashMap<PlotSeriesId, PlotSeries>) };
                    let mut fmt = |name: &str, value: &PlotPoint| {
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
                        macro_rules! axis {
                            ($name:literal, $sym:ident, $other:ident) => {
                                paste! {
                                    ui.menu_button(format!("{}-axis: {}", $name, plot.[<measures_ $sym>].iter().join(", ")), |ui| {
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
                                                        let mut plot = &mut *plot;
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