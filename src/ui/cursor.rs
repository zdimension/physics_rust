use crate::ui::UiState;
use crate::{tools::ToolIcons, UsedMouseButton, FOREGROUND_Z};
use bevy::asset::Handle;
use bevy::math::{Vec2, Vec2Swizzles, Vec3Swizzles};
use bevy::prelude::{
    info, Component, Deref, DerefMut, DetectChanges, DetectChangesMut, Image, Query, Res, ResMut,
    Resource, Transform, UiImage, Visibility, With, Without,
};
use bevy::ui::{Style, Val};
use bevy_egui::EguiContexts;
use bevy_mouse_tracking_plugin::{MainCamera, MousePos, MousePosWorld};

#[derive(Component)]
pub struct ToolCursor;

#[derive(Resource, Deref, DerefMut, PartialEq, Eq, Default)]
pub struct EguiWantsFocus(bool);

pub fn check_egui_wants_focus(mut egui_ctx: EguiContexts, mut wants_focus: ResMut<EguiWantsFocus>) {
    info!(
        "egui wants focus: {:?}",
        egui_ctx.ctx_mut().wants_pointer_input()
    );
    wants_focus.set_if_neq(EguiWantsFocus(egui_ctx.ctx_mut().wants_pointer_input()));
}

pub fn show_current_tool_icon(
    ui_state: Res<UiState>,
    mouse_pos: Res<MousePos>,
    mut icon: Query<(&mut UiImage, &mut Style, &mut Visibility), With<ToolCursor>>,
    tool_icons: Res<ToolIcons>,
    egui_input: Res<EguiWantsFocus>,
) {
    if !(egui_input.is_changed() || mouse_pos.is_changed()) {
        return;
    }
    let (mut icon, mut transform, mut vis) = icon.single_mut();
    if egui_input.0 {
        info!("cursor hidden");
        vis.set_if_neq(Visibility::Hidden);
    } else {
        vis.set_if_neq(Visibility::Visible);
        let current_tool = match ui_state.mouse_button {
            Some(UsedMouseButton::Left) => ui_state.mouse_left,
            Some(UsedMouseButton::Right) => ui_state.mouse_right,
            None => None,
        }
        .unwrap_or(ui_state.toolbox_selected);
        let icon_handle = current_tool.icon(tool_icons);
        icon.texture = icon_handle;
        transform.left = Val::Px(mouse_pos.x);
        transform.top = Val::Px(mouse_pos.y);
    }
}
