use std::collections::HashMap;

use crate::config::AppConfig;
use crate::ui::{PointerToolState, ToolboxState};
use crate::{UsedMouseButton, tools::ToolIcons};

use bevy::asset::{AssetEvent, AssetId, RenderAssetUsages};
use bevy::prelude::{
    AssetServer, Assets, Commands, Component, Deref, DerefMut, DetectChangesMut, Entity, FromWorld,
    Handle, Image, ImageNode, MessageReader, Query, Res, ResMut, Resource, Visibility, With, World,
};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::Node;
use bevy::window::{CursorIcon, CustomCursor, CustomCursorImage, PrimaryWindow, SystemCursorIcon};
use bevy_egui::EguiContexts;

use super::image_processing::{linear_to_srgb, srgb_to_linear};

#[derive(Component)]
pub struct ToolCursor;

#[derive(Resource, Deref, DerefMut, PartialEq, Eq, Default)]
pub struct EguiWantsFocus(bool);

#[derive(Resource)]
pub struct ToolCursorCache {
    arrow: Handle<Image>,
    scale: u32,
    by_source: HashMap<AssetId<Image>, Handle<Image>>,
}

impl FromWorld for ToolCursorCache {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.resource::<AssetServer>();
        Self {
            arrow: asset_server.load("cursors/arrow.png"),
            scale: f32::NAN.to_bits(),
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
    app_config: Res<AppConfig>,
    egui_input: Res<EguiWantsFocus>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    mut images: ResMut<Assets<Image>>,
    mut cursor_cache: ResMut<ToolCursorCache>,
) {
    let (_, _, mut vis) = icon.single_mut().unwrap();
    vis.set_if_neq(Visibility::Hidden);

    for event in image_events.read() {
        let source_id = match event {
            AssetEvent::LoadedWithDependencies { id }
            | AssetEvent::Modified { id }
            | AssetEvent::Removed { id }
            | AssetEvent::Unused { id }
            | AssetEvent::Added { id } => *id,
        };
        if cursor_cache.arrow.id() == source_id {
            cursor_cache.by_source.clear();
        } else if tool_icons.contains_image(source_id) {
            cursor_cache.by_source.remove(&source_id);
        }
    }

    let desired_cursor = if !app_config.tool_cursor || egui_input.0 {
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
            app_config.ui_scale,
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
    ui_scale: f32,
    images: &mut Assets<Image>,
    cache: &mut ToolCursorCache,
) -> Option<Handle<Image>> {
    let source_id = source_handle.id();
    let scale = ui_scale.to_bits();
    if cache.scale != scale {
        cache.scale = scale;
        cache.by_source.clear();
    }
    if let Some(handle) = cache.by_source.get(&source_id) {
        return Some(handle.clone());
    }

    let cursor = {
        let source = images.get(&source_handle)?;
        let arrow = images.get(&cache.arrow)?;
        compose_cursor_image(source, arrow, ui_scale)?
    };
    let handle = images.add(cursor);
    cache.by_source.insert(source_id, handle.clone());
    Some(handle)
}

fn compose_cursor_image(source: &Image, arrow: &Image, ui_scale: f32) -> Option<Image> {
    const WIDTH: u32 = 64;
    const HEIGHT: u32 = 48;
    const ARROW_HEIGHT: u32 = 20;
    const BADGE_SIZE: u32 = 32;
    const BADGE_X: u32 = 16;
    const BADGE_Y: u32 = 12;

    if !is_supported_cursor_source(source)
        || !is_supported_cursor_source(arrow)
        || !ui_scale.is_finite()
        || ui_scale <= 0.0
    {
        return None;
    }
    let width = scaled_dimension(WIDTH, ui_scale);
    let height = scaled_dimension(HEIGHT, ui_scale);
    let arrow_target_height = scaled_dimension(ARROW_HEIGHT, ui_scale);
    let badge_size = scaled_dimension(BADGE_SIZE, ui_scale);
    let badge_x = scaled_coordinate(BADGE_X, ui_scale);
    let badge_y = scaled_coordinate(BADGE_Y, ui_scale);
    let source_data = source.data.as_ref()?;
    let arrow_data = arrow.data.as_ref()?;
    let source_width = source.texture_descriptor.size.width;
    let source_height = source.texture_descriptor.size.height;
    let arrow_width = arrow.texture_descriptor.size.width;
    let arrow_height = arrow.texture_descriptor.size.height;
    if source_width == 0 || source_height == 0 || arrow_width == 0 || arrow_height == 0 {
        return None;
    }

    let mut data = vec![0; (width * height * 4) as usize];
    let arrow_scaled_width =
        ((arrow_width as f32 * arrow_target_height as f32 / arrow_height as f32).round() as u32)
            .max(1);
    blend_scaled_image(
        &mut data,
        width,
        0,
        0,
        arrow_scaled_width,
        arrow_target_height,
        arrow_data,
        arrow_width,
        arrow_height,
    );
    blend_scaled_image(
        &mut data,
        width,
        badge_x,
        badge_y,
        badge_size,
        badge_size,
        source_data,
        source_width,
        source_height,
    );

    Some(Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ))
}

fn scaled_dimension(value: u32, scale: f32) -> u32 {
    (value as f32 * scale).round().max(1.0) as u32
}

fn scaled_coordinate(value: u32, scale: f32) -> u32 {
    (value as f32 * scale).round().max(0.0) as u32
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
            let dst = (((dst_y + y) * dst_width + (dst_x + x)) * 4) as usize;
            if dst + 4 > dst_data.len() {
                continue;
            }
            let sample = sample_bilinear(
                src_data,
                src_width,
                src_height,
                (x as f32 + 0.5) * src_width as f32 / scaled_width as f32 - 0.5,
                (y as f32 + 0.5) * src_height as f32 / scaled_height as f32 - 0.5,
            );
            blend_pixel(&mut dst_data[dst..dst + 4], &sample);
        }
    }
}

fn sample_bilinear(data: &[u8], width: u32, height: u32, x: f32, y: f32) -> [u8; 4] {
    let x = x.clamp(0.0, width.saturating_sub(1) as f32);
    let y = y.clamp(0.0, height.saturating_sub(1) as f32);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let weights = [
        ((1.0 - tx) * (1.0 - ty), x0, y0),
        (tx * (1.0 - ty), x1, y0),
        ((1.0 - tx) * ty, x0, y1),
        (tx * ty, x1, y1),
    ];

    let mut alpha = 0.0;
    let mut premultiplied = [0.0; 3];
    for (weight, sample_x, sample_y) in weights {
        let index = ((sample_y * width + sample_x) * 4) as usize;
        let sample_alpha = data[index + 3] as f32 / 255.0;
        alpha += sample_alpha * weight;
        for channel in 0..3 {
            premultiplied[channel] +=
                srgb_to_linear(data[index + channel] as f32 / 255.0) * sample_alpha * weight;
        }
    }

    let mut result = [0; 4];
    if alpha > f32::EPSILON {
        for channel in 0..3 {
            result[channel] = (linear_to_srgb(premultiplied[channel] / alpha).clamp(0.0, 1.0)
                * 255.0)
                .round() as u8;
        }
    }
    result[3] = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    result
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
        let src_color = srgb_to_linear(src[channel] as f32 / 255.0);
        let dst_color = srgb_to_linear(dst[channel] as f32 / 255.0);
        let out_linear =
            (src_color * src_alpha + dst_color * dst_alpha * (1.0 - src_alpha)) / out_alpha;
        dst[channel] = (linear_to_srgb(out_linear).clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    dst[3] = (out_alpha * 255.0).round() as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_image(width: u32, height: u32, pixel: [u8; 4]) -> Image {
        Image::new(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixel.repeat((width * height) as usize),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        )
    }

    #[test]
    fn cursor_dimensions_follow_fractional_menu_scale() {
        let source = solid_image(2, 2, [255, 255, 255, 255]);
        let arrow = solid_image(1, 1, [255, 255, 255, 255]);

        let cursor = compose_cursor_image(&source, &arrow, 0.83).unwrap();

        assert_eq!(cursor.texture_descriptor.size.width, 53);
        assert_eq!(cursor.texture_descriptor.size.height, 40);
    }

    #[test]
    fn bilinear_resampling_does_not_mix_transparent_rgb_into_visible_edges() {
        let pixels = [255, 0, 0, 255, 0, 0, 255, 0];

        let sample = sample_bilinear(&pixels, 2, 1, 0.5, 0.0);

        assert_eq!(&sample[..3], &[255, 0, 0]);
        assert_eq!(sample[3], 128);
    }
}
