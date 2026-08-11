use std::time::Duration;

use crate::config::AppConfig;
use crate::mouse_tracking::{MainCamera, MousePos, MousePosWorld};
use avian2d::prelude::{AngularVelocity, LinearVelocity, Position, RigidBody, Rotation};
use bevy::ecs::component::Mutable;
use bevy::ecs::query::{QueryData, QueryFilter};
use bevy::log::info;
use bevy::math::{Vec2, Vec2Swizzles, Vec3Swizzles};
use bevy::prelude::*;
use bevy_diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy_egui::egui::load::SizedTexture;
use bevy_egui::egui::{Align2, Context, Id, Pos2, Slider, Ui, pos2};
use bevy_egui::{EguiContexts, egui};
use derivative::Derivative;

use crate::objects::laser::LaserRays;
use crate::palette::{PaletteConfig, PaletteList};
use crate::tools::ToolEnum;
use crate::{UsedMouseButton, egui_systems};

use self::images::GuiIcons;
use self::windows::menu::MenuWindow;

pub mod cursor;
mod custom_widget;
mod icon_button;
pub(crate) mod image_processing;
pub mod images;
mod menu_item;
pub(crate) mod selection_overlay;
mod separator_custom;
mod tabs;
mod text_button;

egui_systems! {
    mod windows,
    mod scale_bar,
    crate::grid::draw_grid,
    ui_example,
    remove_empty_target_windows,
    remove_temporary_windows,
}

pub fn apply_ui_scale(mut egui_ctx: EguiContexts, app_config: Res<AppConfig>) {
    let Ok(ctx) = egui_ctx.ctx_mut() else {
        return;
    };
    let scale = app_config.ui_scale;
    if (ctx.zoom_factor() - scale).abs() > f32::EPSILON {
        ctx.set_zoom_factor(scale);
    }
}

#[derive(Component)]
pub struct Scene;

pub fn ui_example(
    mut egui_ctx: EguiContexts,
    scene_state: Res<SceneState>,
    selected: Query<Entity, With<Selected>>,
    toolbox_state: Res<ToolboxState>,
    pointer_state: Res<PointerToolState>,
    cameras: Query<&mut Transform, With<MainCamera>>,
    mc: Query<Entity, With<MainCamera>>,
    _palette_config: ResMut<PaletteConfig>,
    _assets: Res<Assets<PaletteList>>,
    laser: Query<&LaserRays>,
    mouse: Res<MousePosWorld>,
    mouse_sc: Res<MousePos>,
    diag: Res<DiagnosticsStore>,
) {
    egui::Window::new("Debug").show_translucent(
        egui_ctx.ctx_mut().expect("primary egui context"),
        |ui| {
            ui.collapsing("Mouse", |ui| {
                ui.label(format!("World: {:.2} m", mouse.xy()));
                ui.label(format!("Screen: {:.2} px", mouse_sc.xy()));
            });
            ui.collapsing("Laser", |ui| {
                ui.monospace(&laser.single().unwrap().debug);
            });
            let Ok(tr) = cameras.single() else {
                // dump all components

                panic!("cams found={:#?}", mc.iter().count());
            };
            ui.collapsing("Camera", |ui| {
                ui.monospace(format!(
                    "pos = {:.2} m\nscale = {:.2} m\n",
                    tr.translation, tr.scale
                ));
            });
            ui.collapsing("UI state", |ui| {
                ui.monospace(format!(
                    "{:#?}\n{:#?}\n{:#?}\n{:#?}",
                    selected.iter().collect::<Vec<_>>(),
                    toolbox_state,
                    pointer_state,
                    scene_state
                ));
            });
            /*ui.collapsing("Rapier", |ui| {
                ui.monospace(format!("{:#?}", rapier));
            });*/
            ui.collapsing("FPS", |ui| {
                ui.monospace(format!(
                    "{:.2}",
                    diag.get(&FrameTimeDiagnosticsPlugin::FPS)
                        .unwrap()
                        .value()
                        .unwrap_or(f64::NAN)
                ));
            });
        },
    );
}

trait AsPos2 {
    fn as_pos2(&self) -> Pos2;
}

impl AsPos2 for Vec2 {
    fn as_pos2(&self) -> Pos2 {
        pos2(self.x, self.y)
    }
}

impl AsPos2 for Pos2 {
    fn as_pos2(&self) -> Pos2 {
        *self
    }
}

#[derive(Component, Copy, Clone, Debug)]
pub enum InitialPos {
    Pos(Pos2, Pos2),
    Attached {
        parent: Entity,
        top_offset: f32,
        fallback_anchor: Pos2,
        size: Option<egui::Vec2>,
    },
    ScreenCenter,
}

impl InitialPos {
    fn initial(pos: impl AsPos2) -> impl Bundle {
        (Self::persistent(pos), TemporaryWindow)
    }

    fn persistent(pos: impl AsPos2) -> InitialPos {
        let pos = pos.as_pos2();
        Self::Pos(pos, pos)
    }

    fn attached(parent: Entity, anchor: Pos2, ctx: &Context) -> impl Bundle {
        let top_offset =
            window_rect(ctx, parent).map_or(0.0, |parent_rect| anchor.y - parent_rect.top());
        (
            Self::Attached {
                parent,
                top_offset,
                fallback_anchor: anchor,
                size: None,
            },
            TemporaryWindow,
        )
    }

    /*fn update<T>(&mut self, resp: InnerResponse<T>) {
        self.1 = resp.response.rect.left_top();
    }*/
}

#[derive(Component)]
pub struct TemporaryWindow;

#[derive(Message)]
pub struct ContextMenuEvent {
    pub screen_pos: Vec2,
    pub target: WindowSelectionTarget,
}

#[derive(Component, Clone, Debug, Default)]
pub struct WindowSelectionTarget {
    pub entities: Vec<Entity>,
    title: Option<String>,
}

impl WindowSelectionTarget {
    pub fn from_entities(entities: impl IntoIterator<Item = Entity>) -> Self {
        Self {
            entities: entities.into_iter().fold(Vec::new(), |mut acc, entity| {
                if !acc.contains(&entity) {
                    acc.push(entity);
                }
                acc
            }),
            title: None,
        }
    }

    pub(crate) fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    pub(crate) fn title_or<'a>(&'a self, fallback: &'a str) -> &'a str {
        self.title.as_deref().unwrap_or(fallback)
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = Entity> + '_ {
        self.entities.iter().copied()
    }
}

pub(crate) fn window_title<'a>(
    target: Option<&'a WindowSelectionTarget>,
    fallback: &'a str,
) -> &'a str {
    target.map_or(fallback, |target| target.title_or(fallback))
}

pub(crate) fn window_target_entities(
    target: Option<&WindowSelectionTarget>,
    parent: Option<&ChildOf>,
) -> Vec<Entity> {
    if let Some(target) = target {
        if !target.entities.is_empty() {
            return target.iter().collect();
        }
    }
    parent.map(ChildOf::parent).into_iter().collect::<Vec<_>>()
}

pub(crate) fn window_matching_entities<D: QueryData, F: QueryFilter>(
    target: Option<&WindowSelectionTarget>,
    parent: Option<&ChildOf>,
    query: &Query<D, F>,
) -> Vec<Entity> {
    window_target_entities(target, parent)
        .into_iter()
        .filter(|entity| query.contains(*entity))
        .collect()
}

pub(crate) fn shared_f32(values: impl IntoIterator<Item = f32>) -> Option<f32> {
    let mut values = values.into_iter();
    let first = values.next()?;
    if values.all(|value| value == first) {
        Some(first)
    } else {
        Some(f32::NAN)
    }
}

pub(crate) fn shared_value<T: Copy + PartialEq>(values: impl IntoIterator<Item = T>) -> Option<T> {
    let mut values = values.into_iter();
    let first = values.next()?;
    values.all(|value| value == first).then_some(first)
}

pub(crate) fn max_f32<D, F>(
    targets: &[Entity],
    query: &Query<D, F>,
    get: impl for<'w, 's> Fn(<D as QueryData>::Item<'w, 's>) -> f32,
) -> Option<f32>
where
    D: QueryData<ReadOnly = D>,
    F: QueryFilter,
{
    targets
        .iter()
        .filter_map(|entity| query.get(*entity).ok().map(&get))
        .reduce(f32::max)
}

pub(crate) fn multi_slider<D, F>(
    ui: &mut Ui,
    commands: &mut Commands,
    targets: &[Entity],
    query: &Query<D, F>,
    get: impl for<'w, 's> Fn(<D as QueryData>::Item<'w, 's>) -> f32,
    set: impl for<'w, 's> Fn(Entity, <D as QueryData>::Item<'w, 's>, f32, &mut Commands),
    range: std::ops::RangeInclusive<f32>,
    settings: impl FnOnce(Slider) -> Slider,
) where
    D: QueryData<ReadOnly = D>,
    F: QueryFilter,
{
    let mut current = shared_f32(
        targets
            .iter()
            .filter_map(|entity| query.get(*entity).ok().map(&get)),
    )
    .unwrap_or(f32::NAN);

    if ui
        .add(settings(Slider::new(&mut current, range)).custom())
        .changed()
    {
        for entity in targets {
            if let Ok(item) = query.get(*entity) {
                set(*entity, item, current, commands);
            }
        }
    }
}

pub(crate) fn component_slider<C, F>(
    ui: &mut Ui,
    commands: &mut Commands,
    targets: &[Entity],
    query: &Query<&C, F>,
    get: impl Fn(&C) -> f32,
    set: impl Fn(&mut C, f32),
    range: std::ops::RangeInclusive<f32>,
    settings: impl FnOnce(Slider) -> Slider,
) where
    C: Component + Copy,
    F: QueryFilter,
{
    multi_slider(
        ui,
        commands,
        targets,
        query,
        get,
        |entity, component, value, commands| {
            let mut component = *component;
            set(&mut component, value);
            commands.entity(entity).insert(component);
        },
        range,
        settings,
    );
}

pub(crate) fn edit_components<C>(
    commands: &mut Commands,
    targets: &[Entity],
    query: &Query<&C>,
    edit: impl Fn(&mut C),
) where
    C: Component + Copy,
{
    for entity in targets {
        if let Ok(component) = query.get(*entity) {
            let mut component = *component;
            edit(&mut component);
            commands.entity(*entity).insert(component);
        }
    }
}

pub(crate) fn component_slider_mut<C>(
    ui: &mut Ui,
    targets: &[Entity],
    query: &mut Query<&mut C>,
    get: impl Fn(&C) -> f32,
    set: impl Fn(&mut C, f32),
    range: std::ops::RangeInclusive<f32>,
    settings: impl FnOnce(Slider) -> Slider,
) where
    C: Component<Mutability = Mutable>,
{
    let mut values = Vec::new();
    for entity in targets {
        if let Ok(component) = query.get_mut(*entity) {
            values.push(get(&component));
        }
    }
    let mut current = shared_f32(values).unwrap_or(f32::NAN);

    if ui
        .add(settings(Slider::new(&mut current, range)).custom())
        .changed()
    {
        for entity in targets {
            if let Ok(mut component) = query.get_mut(*entity) {
                set(&mut *component, current);
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TriState {
    Off,
    On,
    Mixed,
}

pub(crate) fn shared_bool(values: impl IntoIterator<Item = bool>) -> Option<TriState> {
    let mut values = values.into_iter();
    let first = values.next()?;
    if values.all(|value| value == first) {
        Some(if first { TriState::On } else { TriState::Off })
    } else {
        Some(TriState::Mixed)
    }
}

pub(crate) fn component_checkbox<C>(
    ui: &mut Ui,
    commands: &mut Commands,
    icons: &GuiIcons,
    targets: &[Entity],
    query: &Query<&C>,
    get: impl Fn(&C) -> bool,
    set: impl Fn(&mut C, bool),
    label: impl Into<egui::WidgetText>,
) -> Option<TriState>
where
    C: Component + Copy,
{
    let state = shared_bool(
        targets
            .iter()
            .filter_map(|entity| query.get(*entity).ok().map(&get)),
    )?;

    if let Some(value) = image_checkbox(ui, icons, state, label) {
        for entity in targets {
            if let Ok(component) = query.get(*entity) {
                let mut component = *component;
                set(&mut component, value);
                commands.entity(*entity).insert(component);
            }
        }
    }

    Some(state)
}

pub(crate) fn image_checkbox(
    ui: &mut Ui,
    icons: &GuiIcons,
    state: TriState,
    label: impl Into<egui::WidgetText>,
) -> Option<bool> {
    let icon = match state {
        TriState::Off => icons.checkbox_off,
        TriState::On => icons.checkbox_on,
        TriState::Mixed => icons.checkbox_unknown,
    };
    let clicked = image_labeled_button(ui, icon, label);
    clicked.then_some(!matches!(state, TriState::On))
}

fn image_labeled_button(
    ui: &mut Ui,
    icon: egui::TextureId,
    label: impl Into<egui::WidgetText>,
) -> bool {
    ui.add(egui::Button::image_and_text(
        SizedTexture::new(icon, [16.0, 16.0]),
        label,
    ))
    .clicked()
}

pub(crate) fn image_radio(
    ui: &mut Ui,
    icons: &GuiIcons,
    selected: bool,
    label: impl Into<egui::WidgetText>,
) -> bool {
    image_labeled_button(
        ui,
        if selected {
            icons.radio_on
        } else {
            icons.radio_off
        },
        label,
    )
}

pub(crate) fn bool_checkbox(
    ui: &mut Ui,
    icons: &GuiIcons,
    value: &mut bool,
    label: impl Into<egui::WidgetText>,
) -> bool {
    let state = if *value { TriState::On } else { TriState::Off };
    if let Some(new_value) = image_checkbox(ui, icons, state, label) {
        *value = new_value;
        true
    } else {
        false
    }
}

pub fn handle_context_menu(
    mut ev: MessageReader<ContextMenuEvent>,
    mut commands: Commands,
    existing: Query<Entity, With<MenuWindow>>,
) {
    for ev in ev.read() {
        info!("context menu at {:?} for {:?}", ev.screen_pos, ev.target);
        if let Ok(existing) = existing.single() {
            commands.entity(existing).despawn();
        }
        commands.spawn((
            MenuWindow::default(),
            ev.target.clone(),
            InitialPos::initial(ev.screen_pos),
        ));
    }
}

fn remove_empty_target_windows(
    mut commands: Commands,
    wnds: Query<(Entity, &WindowSelectionTarget)>,
    alive: Query<()>,
) {
    for (entity, target) in &wnds {
        if !target.entities.is_empty()
            && !target.entities.iter().any(|entity| alive.contains(*entity))
        {
            commands.entity(entity).despawn();
        }
    }
}

trait BevyIdThing {
    fn id_bevy(self, id: Entity) -> Self;
}

impl<'a> BevyIdThing for egui::Window<'a> {
    fn id_bevy(self, id: Entity) -> Self {
        self.id(Id::new(id))
    }
}

fn window_rect(ctx: &Context, id: Entity) -> Option<egui::Rect> {
    let id = Id::new(id);
    ctx.read_response(id.with("move"))
        .map(|response| response.rect)
        .or_else(|| ctx.memory(|memory| memory.area_rect(id)))
}

fn attached_window_position(
    parent_rect: egui::Rect,
    child_size: Option<egui::Vec2>,
    content_rect: egui::Rect,
    top_offset: f32,
) -> Pos2 {
    let fits_on_right = child_size
        .is_none_or(|size| parent_rect.right() + size.x <= content_rect.right() + f32::EPSILON);
    let x = if fits_on_right {
        parent_rect.right()
    } else {
        parent_rect.left() - child_size.unwrap().x
    };
    pos2(x, parent_rect.top() + top_offset)
}

/*impl Into<Pos2> for &InitialPos {
    fn into(self) -> Pos2 {
        self.0
    }
}*/

trait Subwindow {
    fn subwindow(
        self,
        id: Entity,
        ctx: &Context,
        initial_pos: &mut InitialPos,
        commands: &mut Commands,
        contents: impl FnOnce(&mut Ui, &mut Commands),
    );
}

impl<'a> Subwindow for egui::Window<'a> {
    fn subwindow(
        self,
        id: Entity,
        ctx: &Context,
        initial_pos: &mut InitialPos,
        commands: &mut Commands,
        contents: impl FnOnce(&mut Ui, &mut Commands),
    ) {
        let mut open = true;
        let center = ctx.input(|i| i.content_rect().size()) / 2.0;
        let (wnd, begin) = match *initial_pos {
            InitialPos::Pos(begin, _) => (self.pivot(Align2::LEFT_TOP).default_pos(begin), begin),
            InitialPos::Attached {
                parent,
                top_offset,
                fallback_anchor,
                size,
            } => {
                let content_rect = ctx.input(|input| input.content_rect());
                let pos = window_rect(ctx, parent).map_or(fallback_anchor, |parent_rect| {
                    attached_window_position(parent_rect, size, content_rect, top_offset)
                });
                (self.pivot(Align2::LEFT_TOP).current_pos(pos), pos)
            }
            InitialPos::ScreenCenter => {
                /*let input = ctx.input(|i| i.screen_rect);*/

                let zero = center.to_pos2();
                // TODO !!
                (self, zero)
            }
        };
        let response = wnd
            .id_bevy(id)
            .open(&mut open)
            .show_translucent(ctx, |ui| contents(ui, commands))
            .map(|resp| resp.response);
        if let Some(response) = response {
            let current = response.rect.left_top();
            if response.dragged() {
                *initial_pos = InitialPos::Pos(current, current);
                commands.entity(id).remove::<TemporaryWindow>();
            } else {
                match initial_pos {
                    InitialPos::Pos(_, stored_current) => *stored_current = current,
                    InitialPos::Attached { size, .. } => *size = Some(response.rect.size()),
                    InitialPos::ScreenCenter => {
                        *initial_pos = InitialPos::Pos(begin, current);
                    }
                }
            }
        }
        if !open {
            info!("closing window");
            commands.entity(id).despawn();
        }
    }
}

#[derive(Message)]
pub struct RemoveTemporaryWindowsEvent;

fn remove_temporary_windows(
    mut commands: Commands,
    mut events: MessageReader<RemoveTemporaryWindowsEvent>,
    wnds: Query<Entity, With<TemporaryWindow>>,
) {
    for _ in events.read() {
        for id in wnds.iter() {
            commands.entity(id).despawn();
        }
    }
}

#[derive(Component, Copy, Clone, Debug, Default)]
pub struct Selected;

#[derive(Resource, Derivative)]
#[derivative(Debug)]
pub struct ToolboxState {
    #[derivative(Debug = "ignore")]
    toolbox: Vec<Vec<ToolEnum>>,
    #[derivative(Debug = "ignore")]
    toolbox_bottom: Vec<ToolEnum>,
    pub toolbox_selected: ToolEnum,
}

impl Default for ToolboxState {
    fn default() -> Self {
        macro_rules! tool {
            ($ty:ident) => {
                ToolEnum::$ty(Default::default())
            };
        }

        let pan = tool!(Pan);

        Self {
            toolbox: vec![
                vec![tool!(Move), tool!(Drag), tool!(Rotate)],
                vec![
                    tool!(Polygon),
                    tool!(Gear),
                    tool!(Box),
                    tool!(Circle),
                    tool!(Plane),
                ],
                vec![
                    tool!(Spring),
                    tool!(Fix),
                    tool!(Axle),
                    tool!(Thruster),
                    tool!(Laser),
                    tool!(Tracer),
                ],
            ],
            toolbox_bottom: vec![tool!(Zoom), pan.clone()],
            toolbox_selected: pan,
        }
    }
}

#[derive(Resource, Derivative)]
#[derivative(Debug)]
pub struct PointerToolState {
    pub mouse_left: Option<ToolEnum>,
    pub mouse_left_pos: Option<(Duration, Vec2, Vec2)>,
    pub mouse_right: Option<ToolEnum>,
    pub mouse_right_pos: Option<(Duration, Vec2, Vec2)>,
    pub mouse_button: Option<UsedMouseButton>,
}

impl Default for PointerToolState {
    fn default() -> Self {
        Self {
            mouse_left: None,
            mouse_left_pos: None,
            mouse_right: None,
            mouse_right_pos: None,
            mouse_button: None,
        }
    }
}

#[derive(Resource, Derivative)]
#[derivative(Debug)]
pub struct SceneState {
    pub scene: Entity,
    pub sky: Entity,
}

impl FromWorld for SceneState {
    fn from_world(world: &mut World) -> Self {
        let scene = world
            .spawn((
                Scene,
                Transform::default(),
                Visibility::Inherited,
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ))
            .id();
        let sky = world
            .spawn((
                crate::objects::body::PhysicsBody,
                RigidBody::Static,
                Position::default(),
                Rotation::default(),
                LinearVelocity::ZERO,
                AngularVelocity::ZERO,
                Transform::default(),
                GlobalTransform::default(),
                ChildOf(scene),
            ))
            .id();
        Self { scene, sky }
    }
}

pub trait WindowExt {
    fn show_translucent<R>(
        self,
        ctx: &egui::Context,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> Option<egui::InnerResponse<Option<R>>>;
}

impl<'a> WindowExt for egui::Window<'a> {
    fn show_translucent<R>(
        self,
        ctx: &egui::Context,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> Option<egui::InnerResponse<Option<R>>> {
        let opacity = 0.925;

        let frame = egui::Frame::window(&ctx.global_style()).multiply_with_opacity(opacity);

        #[allow(clippy::disallowed_methods)]
        self.drag_area(egui::WindowDrag::Anywhere)
            .frame(frame)
            .show(ctx, |ui| {
                ui.multiply_opacity(opacity);
                add_contents(ui)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attached_window_is_placed_on_the_right_when_it_fits() {
        let parent = egui::Rect::from_min_size(pos2(100.0, 40.0), egui::vec2(80.0, 200.0));
        let screen = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(500.0, 400.0));

        let pos = attached_window_position(parent, Some(egui::vec2(150.0, 100.0)), screen, 35.0);

        assert_eq!(pos, pos2(parent.right(), parent.top() + 35.0));
    }

    #[test]
    fn attached_window_moves_to_the_left_when_the_right_is_too_narrow() {
        let parent = egui::Rect::from_min_size(pos2(300.0, 40.0), egui::vec2(80.0, 200.0));
        let screen = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(500.0, 400.0));
        let child_size = egui::vec2(150.0, 100.0);

        let pos = attached_window_position(parent, Some(child_size), screen, 35.0);

        assert_eq!(pos, pos2(parent.left() - child_size.x, parent.top() + 35.0));
    }

    #[test]
    fn attached_window_tracks_parent_movement() {
        let parent = egui::Rect::from_min_size(pos2(100.0, 40.0), egui::vec2(80.0, 200.0));
        let moved_parent = parent.translate(egui::vec2(25.0, 15.0));
        let screen = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(500.0, 400.0));
        let size = Some(egui::vec2(150.0, 100.0));

        let initial = attached_window_position(parent, size, screen, 35.0);
        let moved = attached_window_position(moved_parent, size, screen, 35.0);

        assert_eq!(moved - initial, egui::vec2(25.0, 15.0));
    }

    #[test]
    fn deferred_numeric_edits_do_not_change_or_signal_until_focus_is_lost() {
        let ctx = egui::Context::default();
        let mut value = 1.0_f64;
        let mut editor_id = egui::Id::NULL;

        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let response = ui.add(egui::DragValue::new(&mut value).update_while_editing(false));
            editor_id = response.id;
            response.request_focus();
        });

        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::Text("2".into()));
        let mut draft_reported_changed = false;
        let _ = ctx.run_ui(input, |ui| {
            draft_reported_changed = ui
                .add(egui::DragValue::new(&mut value).update_while_editing(false))
                .changed();
        });

        assert_eq!(value, 1.0);
        assert!(!draft_reported_changed);

        ctx.memory_mut(|memory| memory.surrender_focus(editor_id));
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.add(egui::DragValue::new(&mut value).update_while_editing(false));
        });
        assert_eq!(value, 12.0);
    }

    #[test]
    fn sliders_allow_out_of_range_text_values_by_default() {
        assert_eq!(egui::SliderClamping::default(), egui::SliderClamping::Never);
    }
}
