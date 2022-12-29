use bevy::prelude::{Component, Image, Query, Res, ResMut, Transform, Visibility, With, Without};
use bevy_mouse_tracking_plugin::{MainCamera, MousePosWorld};
use bevy::asset::Handle;
use bevy_egui::EguiContext;
use bevy::math::{Vec2, Vec3Swizzles};
use crate::{FOREGROUND_Z, ToolIcons, UiState, UsedMouseButton};

#[derive(Component)]
pub struct ToolCursor;

pub fn show_current_tool_icon(
    ui_state: Res<UiState>,
    mouse_pos: Res<MousePosWorld>,
    mut icon: Query<(&mut Handle<Image>, &mut Transform, &mut Visibility), With<ToolCursor>>,
    camera: Query<&Transform, (With<MainCamera>, Without<ToolCursor>)>,
    tool_icons: Res<ToolIcons>,
    mut egui_ctx: ResMut<EguiContext>,
) {
    let (mut icon, mut transform, mut vis) = icon.single_mut();
    if egui_ctx.ctx_mut().wants_pointer_input() {
        *vis = Visibility::INVISIBLE;
    } else {
        *vis = Visibility::VISIBLE;
        let current_tool = match ui_state.mouse_button {
            Some(UsedMouseButton::Left) => ui_state.mouse_left,
            Some(UsedMouseButton::Right) => ui_state.mouse_right,
            None => None,
        }
        .unwrap_or(ui_state.toolbox_selected);
        let icon_handle = current_tool.icon(tool_icons);
        let cam_scale = camera.single().scale.xy();
        *icon = icon_handle;
        transform.translation =
            (mouse_pos.xy() + cam_scale * 30.0 * Vec2::new(1.0, -1.0)).extend(FOREGROUND_Z);
        transform.scale = (cam_scale * 0.26).extend(1.0);
    }
}
