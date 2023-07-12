use crate::ui::UiState;
use crate::{tools::ToolIcons, UsedMouseButton, FOREGROUND_Z};
use bevy::asset::Handle;
use bevy::math::{Vec2, Vec2Swizzles, Vec3Swizzles};
use bevy::prelude::{Component, Image, Query, Res, Transform, UiImage, Visibility, With, Without};
use bevy::ui::{Style, Val};
use bevy_egui::{EguiContexts};
use bevy_mouse_tracking_plugin::{MainCamera, MousePos, MousePosWorld};

#[derive(Component)]
pub struct ToolCursor;

pub fn show_current_tool_icon(
    ui_state: Res<UiState>,
    mouse_pos: Res<MousePos>,
    mut icon: Query<(&mut UiImage, &mut Style, &mut Visibility), With<ToolCursor>>,
    camera: Query<&Transform, (With<MainCamera>, Without<ToolCursor>)>,
    tool_icons: Res<ToolIcons>,
    mut egui_ctx: EguiContexts,
) {
    let (mut icon, mut transform, mut vis) = icon.single_mut();
    if egui_ctx.ctx_mut().wants_pointer_input() {
        *vis = Visibility::Hidden;
    } else {
        *vis = Visibility::Visible;
        let current_tool = match ui_state.mouse_button {
            Some(UsedMouseButton::Left) => ui_state.mouse_left,
            Some(UsedMouseButton::Right) => ui_state.mouse_right,
            None => None,
        }
        .unwrap_or(ui_state.toolbox_selected);
        let icon_handle = current_tool.icon(tool_icons);
        let cam_scale = camera.single().scale.xy();
        icon.texture = icon_handle;
        /*transform.translation =
            (mouse_pos.xy() + cam_scale * 30.0 * Vec2::new(1.0, -1.0)).extend(FOREGROUND_Z);
        transform.scale = (cam_scale * 0.26).extend(1.0);*/
        transform.left = Val::Px(mouse_pos.x);
        transform.top = Val::Px(mouse_pos.y);
        //transform.translation = (mouse_pos.xy() * Vec2::new(1.0, -1.0)).extend(1.0);
    }
}
