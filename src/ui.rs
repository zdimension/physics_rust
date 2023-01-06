use crate::{demo, PaletteConfig, UiState};
use bevy::log::info;
use bevy::math::{Vec2, Vec3, Vec3Swizzles};
use bevy::prelude::*;
use bevy_egui::egui::{pos2, Context, Id, InnerResponse, Pos2, Ui};
use bevy_egui::{egui, EguiContext};
use bevy_mouse_tracking_plugin::{MainCamera, MousePosWorld};

use derivative::Derivative;

mod icon_button;
mod menu_item;
mod scene_actions;
mod separator_custom;
mod toolbox;
mod windows;
mod toolbar;

use self::windows::collisions::CollisionsWindow;
use self::windows::information::InformationWindow;
use self::windows::menu::MenuWindow;
use crate::objects::laser::LaserRays;
use crate::ui::windows::laser::LaserWindow;
use windows::plot::PlotWindow;
use crate::palette::PaletteList;
use crate::ui::windows::appearance::AppearanceWindow;
use crate::ui::windows::material::MaterialWindow;

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
    mut palette_config: ResMut<PaletteConfig>,
    assets: Res<Assets<PaletteList>>,
    laser: Query<&LaserRays>,
    mut cmds: Commands,
    mouse: Res<MousePosWorld>
) {
    if !*is_initialized {


        palette_config.current_palette = *assets.get(&palette_config.palettes).unwrap().0.get("Optics").unwrap();

        demo::lasers::init(&mut cmds);
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
        let tr = cameras.single_mut();
        ui.monospace(format!(
            "Mouse: {:.2} m\n\
            {}\n\
            {:#?}\n\
            pos = {:.2} m\n\
            scale = {:.2} m\n",
            mouse.xy(),
            laser.single().debug,
            ui_state,
            tr.translation,
            tr.scale
        ));
    });
}

pub fn draw_ui() -> SystemSet {
    SystemSet::new()
        .with_system(ui_example)
        .with_system(toolbox::draw_toolbox)
        .with_system(toolbar::draw_bottom_toolbar)
        .with_system(scene_actions::draw_scene_actions)
        .with_system(process_temporary_windows)
        .with_system(remove_temporary_windows)
        .with_system(MenuWindow::show)
        .with_system(InformationWindow::show)
        .with_system(PlotWindow::show)
        .with_system(CollisionsWindow::show)
        .with_system(LaserWindow::show)
        .with_system(MaterialWindow::show)
        .with_system(AppearanceWindow::show)
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
pub struct InitialPos(Pos2, Pos2);

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
    ui: ResMut<UiState>,
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
        // todo: really detect whether window was moved
        if pos.0.distance(pos.1) > 1.0 {
            info!(
                "marking window {:?} as persistent (initial {:?} != current {:?})",
                wnd, pos.0, pos.1
            );
            commands.entity(wnd).remove::<TemporaryWindow>();
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

impl Into<Pos2> for &InitialPos {
    fn into(self) -> Pos2 {
        self.0
    }
}

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
        self.id_bevy(id)
            .default_pos(&*initial_pos)
            .open(&mut open)
            .show(ctx, |ui| contents(ui, commands))
            .map(|resp| initial_pos.1 = resp.response.rect.left_top());
        if !open {
            info!("closing window");
            commands.entity(id).despawn_recursive();
        }
    }
}

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
