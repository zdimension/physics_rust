use std::collections::HashMap;

use crate::ui::{PointerToolState, ToolboxState};
use crate::{UsedMouseButton, tools::ToolIcons};

use bevy::asset::{AssetId, RenderAssetUsages};
use bevy::prelude::{
    AssetServer, Assets, Commands, Component, Deref, DerefMut, DetectChangesMut, Entity, FromWorld,
    Handle, Image, ImageNode, Query, Res, ResMut, Resource, Visibility, With, World,
};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::Node;
use bevy::window::{CursorIcon, CustomCursor, CustomCursorImage, PrimaryWindow, SystemCursorIcon};
use bevy_egui::EguiContexts;

#[derive(Component)]
pub struct ToolCursor;

#[derive(Resource, Deref, DerefMut, PartialEq, Eq, Default)]
pub struct EguiWantsFocus(bool);

#[derive(Resource)]
pub struct ToolCursorCache {
    arrow: Handle<Image>,
    by_source: HashMap<AssetId<Image>, Handle<Image>>,
}

impl FromWorld for ToolCursorCache {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.resource::<AssetServer>();
        Self {
            arrow: asset_server.load("cursors/arrow.png"),
            by_source: HashMap::new(),
        }
    }
}

pub fn check_egui_wants_focus(mut egui_ctx: EguiContexts, mut wants_focus: ResMut<EguiWantsFocus>) {
    let ctx = egui_ctx.ctx_mut().expect("primary egui context");
    wants_focus.set_if_neq(EguiWantsFocus(
        ctx.egui_is_using_pointer() || ctx.is_pointer_over_egui(),
    ));
}

pub fn show_current_tool_icon(
    pointer_state: Res<PointerToolState>,
    toolbox_state: Res<ToolboxState>,
    mut commands: Commands,
    mut icon: Query<(&mut ImageNode, &mut Node, &mut Visibility), With<ToolCursor>>,
    window: Query<(Entity, Option<&CursorIcon>), With<PrimaryWindow>>,
    tool_icons: Res<ToolIcons>,
    egui_input: Res<EguiWantsFocus>,
    mut images: ResMut<Assets<Image>>,
    mut cursor_cache: ResMut<ToolCursorCache>,
) {
    let (_, _, mut vis) = icon.single_mut().unwrap();
    vis.set_if_neq(Visibility::Hidden);

    let desired_cursor = if egui_input.0 {
        CursorIcon::System(SystemCursorIcon::Default)
    } else {
        let current_tool = match pointer_state.mouse_button {
            Some(UsedMouseButton::Left) => pointer_state.mouse_left.clone(),
            Some(UsedMouseButton::Right) => pointer_state.mouse_right.clone(),
            None => None,
        }
        .unwrap_or_else(|| toolbox_state.toolbox_selected.clone());

        match hardware_cursor_for_tool(
            current_tool.icon(tool_icons),
            &mut images,
            &mut cursor_cache,
        ) {
            Some(handle) => CursorIcon::Custom(CustomCursor::Image(CustomCursorImage {
                handle,
                hotspot: (0, 0),
                ..Default::default()
            })),
            None => CursorIcon::System(SystemCursorIcon::Default),
        }
    };

    let Ok((window, current_cursor)) = window.single() else {
        return;
    };
    if current_cursor != Some(&desired_cursor) {
        commands.entity(window).insert(desired_cursor);
    }
}

fn hardware_cursor_for_tool(
    source_handle: Handle<Image>,
    images: &mut Assets<Image>,
    cache: &mut ToolCursorCache,
) -> Option<Handle<Image>> {
    let source_id = source_handle.id();
    if let Some(handle) = cache.by_source.get(&source_id) {
        return Some(handle.clone());
    }

    let cursor = {
        let source = images.get(&source_handle)?;
        let arrow = images.get(&cache.arrow)?;
        compose_cursor_image(source, arrow)?
    };
    let handle = images.add(cursor);
    cache.by_source.insert(source_id, handle.clone());
    Some(handle)
}

fn compose_cursor_image(source: &Image, arrow: &Image) -> Option<Image> {
    const WIDTH: u32 = 64;
    const HEIGHT: u32 = 48;
    const ARROW_HEIGHT: u32 = 20;
    const BADGE_SIZE: u32 = 32;
    const BADGE_X: u32 = 16;
    const BADGE_Y: u32 = 12;

    if !is_supported_cursor_source(source) || !is_supported_cursor_source(arrow) {
        return None;
    }
    let source_data = source.data.as_ref()?;
    let arrow_data = arrow.data.as_ref()?;
    let source_width = source.texture_descriptor.size.width;
    let source_height = source.texture_descriptor.size.height;
    let arrow_width = arrow.texture_descriptor.size.width;
    let arrow_height = arrow.texture_descriptor.size.height;
    if source_width == 0 || source_height == 0 || arrow_width == 0 || arrow_height == 0 {
        return None;
    }

    let mut data = vec![0; (WIDTH * HEIGHT * 4) as usize];
    let arrow_scaled_width = (arrow_width * ARROW_HEIGHT / arrow_height).max(1);
    blend_scaled_image(
        &mut data,
        WIDTH,
        0,
        0,
        arrow_scaled_width,
        ARROW_HEIGHT,
        arrow_data,
        arrow_width,
        arrow_height,
    );
    blend_scaled_image(
        &mut data,
        WIDTH,
        BADGE_X,
        BADGE_Y,
        BADGE_SIZE,
        BADGE_SIZE,
        source_data,
        source_width,
        source_height,
    );

    Some(Image::new(
        Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ))
}

fn is_supported_cursor_source(image: &Image) -> bool {
    matches!(
        image.texture_descriptor.format,
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb
    )
}

fn blend_scaled_image(
    dst_data: &mut [u8],
    dst_width: u32,
    dst_x: u32,
    dst_y: u32,
    scaled_width: u32,
    scaled_height: u32,
    src_data: &[u8],
    src_width: u32,
    src_height: u32,
) {
    for y in 0..scaled_height {
        for x in 0..scaled_width {
            let sx = x * src_width / scaled_width;
            let sy = y * src_height / scaled_height;
            let src = ((sy * src_width + sx) * 4) as usize;
            let dst = (((dst_y + y) * dst_width + (dst_x + x)) * 4) as usize;
            blend_pixel(&mut dst_data[dst..dst + 4], &src_data[src..src + 4]);
        }
    }
}

fn blend_pixel(dst: &mut [u8], src: &[u8]) {
    let src_alpha = src[3] as f32 / 255.0;
    let dst_alpha = dst[3] as f32 / 255.0;
    let out_alpha = src_alpha + dst_alpha * (1.0 - src_alpha);
    if out_alpha <= f32::EPSILON {
        dst.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }

    for channel in 0..3 {
        let src_color = src[channel] as f32 / 255.0;
        let dst_color = dst[channel] as f32 / 255.0;
        let out_color =
            (src_color * src_alpha + dst_color * dst_alpha * (1.0 - src_alpha)) / out_alpha;
        dst[channel] = (out_color * 255.0).round() as u8;
    }
    dst[3] = (out_alpha * 255.0).round() as u8;
}
