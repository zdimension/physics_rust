use std::collections::{HashSet, VecDeque};

use bevy::asset::AssetId;
use bevy::image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{TextureDimension, TextureFormat};

use crate::tools::ToolIcons;

use super::images::{AppIcons, GuiIcons};

#[derive(Resource, Default)]
pub(crate) struct ImagePreparationState {
    self_modified: HashSet<AssetId<Image>>,
}

pub(crate) fn prepare_images(
    tool_icons: Res<ToolIcons>,
    gui_icons: Res<GuiIcons>,
    app_icons: Res<AppIcons>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    mut images: ResMut<Assets<Image>>,
    mut state: ResMut<ImagePreparationState>,
) {
    for event in image_events.read() {
        let (source_id, modified) = match event {
            AssetEvent::LoadedWithDependencies { id } => (*id, false),
            AssetEvent::Modified { id } => (*id, true),
            _ => continue,
        };

        if modified && state.self_modified.remove(&source_id) {
            continue;
        }

        if let Some(derived) = tool_icons
            .egui_image_for_source(source_id)
            .or_else(|| gui_icons.egui_image_for_source(source_id))
        {
            let prepared = images
                .get(source_id)
                .and_then(|source| prepare_image(source, true));
            if let Some(prepared) = prepared {
                let _ = images.insert(derived.id(), prepared);
            }
        }

        if app_icons.contains_image(source_id) {
            let prepared = images
                .get(source_id)
                .and_then(|source| prepare_image(source, false));
            if let Some(prepared) = prepared
                && images.insert(source_id, prepared).is_ok()
            {
                state.self_modified.insert(source_id);
            }
        }
    }
}

pub(crate) fn prepare_image(source: &Image, premultiply: bool) -> Option<Image> {
    if source.texture_descriptor.dimension != TextureDimension::D2
        || source.texture_descriptor.size.depth_or_array_layers != 1
        || !matches!(
            source.texture_descriptor.format,
            TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb
        )
    {
        return None;
    }

    let width = source.texture_descriptor.size.width;
    let height = source.texture_descriptor.size.height;
    if width == 0 || height == 0 {
        return None;
    }
    let base_len = rgba_len(width, height)?;
    let source_data = source.data.as_ref()?;
    let mut level = source_data.get(..base_len)?.to_vec();
    bleed_transparent_rgb(&mut level, width, height);

    let mut data = Vec::with_capacity(full_mip_len(width, height)?);
    data.extend_from_slice(&level);
    let mut level_width = width;
    let mut level_height = height;
    let mut mip_level_count = 1;

    while level_width > 1 || level_height > 1 {
        let next_width = (level_width / 2).max(1);
        let next_height = (level_height / 2).max(1);
        level = downsample_rgba(&level, level_width, level_height, next_width, next_height);
        bleed_transparent_rgb(&mut level, next_width, next_height);
        data.extend_from_slice(&level);
        level_width = next_width;
        level_height = next_height;
        mip_level_count += 1;
    }

    if premultiply {
        premultiply_rgba(&mut data);
    }

    let mut prepared = source.clone();
    prepared.data = Some(data);
    prepared.texture_descriptor.mip_level_count = mip_level_count;
    prepared.sampler = if premultiply {
        let mut sampler = ImageSamplerDescriptor::linear();
        sampler.mipmap_filter = ImageFilterMode::Nearest;
        ImageSampler::Descriptor(sampler)
    } else {
        ImageSampler::linear()
    };
    Some(prepared)
}

fn rgba_len(width: u32, height: u32) -> Option<usize> {
    usize::try_from(width.checked_mul(height)?.checked_mul(4)?).ok()
}

fn full_mip_len(mut width: u32, mut height: u32) -> Option<usize> {
    let mut len = 0usize;
    loop {
        len = len.checked_add(rgba_len(width, height)?)?;
        if width == 1 && height == 1 {
            return Some(len);
        }
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
}

fn downsample_rgba(
    source: &[u8],
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> Vec<u8> {
    let mut target = vec![0; rgba_len(target_width, target_height).unwrap_or(0)];
    for target_y in 0..target_height {
        let source_y0 = target_y * source_height / target_height;
        let source_y1 = ((target_y + 1) * source_height / target_height).max(source_y0 + 1);
        for target_x in 0..target_width {
            let source_x0 = target_x * source_width / target_width;
            let source_x1 = ((target_x + 1) * source_width / target_width).max(source_x0 + 1);
            let mut alpha_sum = 0.0;
            let mut linear_premultiplied = [0.0; 3];
            let mut sample_count = 0.0;

            for source_y in source_y0..source_y1.min(source_height) {
                for source_x in source_x0..source_x1.min(source_width) {
                    let index = ((source_y * source_width + source_x) * 4) as usize;
                    let alpha = source[index + 3] as f32 / 255.0;
                    alpha_sum += alpha;
                    sample_count += 1.0;
                    for channel in 0..3 {
                        linear_premultiplied[channel] +=
                            srgb_to_linear(source[index + channel] as f32 / 255.0) * alpha;
                    }
                }
            }

            let target_index = ((target_y * target_width + target_x) * 4) as usize;
            if alpha_sum > f32::EPSILON {
                for channel in 0..3 {
                    let linear = linear_premultiplied[channel] / alpha_sum;
                    target[target_index + channel] =
                        (linear_to_srgb(linear).clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
            target[target_index + 3] =
                ((alpha_sum / sample_count).clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    target
}

fn bleed_transparent_rgb(data: &mut [u8], width: u32, height: u32) {
    let pixel_count = (width * height) as usize;
    if data.len() < pixel_count * 4 {
        return;
    }

    let mut visited = vec![false; pixel_count];
    let mut queue = VecDeque::new();
    for index in 0..pixel_count {
        if data[index * 4 + 3] != 0 {
            visited[index] = true;
            queue.push_back(index);
        }
    }

    while let Some(index) = queue.pop_front() {
        let x = index as u32 % width;
        let y = index as u32 / width;
        for (next_x, next_y) in [
            x.checked_sub(1).map(|x| (x, y)),
            (x + 1 < width).then_some((x + 1, y)),
            y.checked_sub(1).map(|y| (x, y)),
            (y + 1 < height).then_some((x, y + 1)),
        ]
        .into_iter()
        .flatten()
        {
            let next = (next_y * width + next_x) as usize;
            if visited[next] {
                continue;
            }
            visited[next] = true;
            let source = index * 4;
            let target = next * 4;
            let rgb = [data[source], data[source + 1], data[source + 2]];
            data[target..target + 3].copy_from_slice(&rgb);
            queue.push_back(next);
        }
    }
}

fn premultiply_rgba(data: &mut [u8]) {
    for pixel in data.chunks_exact_mut(4) {
        let alpha = pixel[3] as u16;
        for component in &mut pixel[..3] {
            *component = ((*component as u16 * alpha + 127) / 255) as u8;
        }
    }
}

pub(super) fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

pub(super) fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::Extent3d;

    fn image(width: u32, height: u32, data: Vec<u8>) -> Image {
        Image::new(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        )
    }

    #[test]
    fn builds_a_complete_mip_chain_with_contiguous_levels() {
        let prepared = prepare_image(&image(4, 2, vec![255; 4 * 2 * 4]), false).unwrap();

        assert_eq!(prepared.texture_descriptor.mip_level_count, 3);
        assert_eq!(prepared.data.unwrap().len(), 32 + 8 + 4);
        assert_eq!(prepared.sampler, ImageSampler::linear());
    }

    #[test]
    fn downsampling_weights_color_by_alpha_in_linear_light() {
        let source = image(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 0]);
        let prepared = prepare_image(&source, false).unwrap();
        let mip = &prepared.data.unwrap()[8..12];

        assert_eq!(&mip[..3], &[255, 0, 0]);
        assert_eq!(mip[3], 128);
    }

    #[test]
    fn bleeds_edge_color_into_transparent_pixels_for_straight_alpha_images() {
        let prepared =
            prepare_image(&image(2, 1, vec![20, 40, 80, 255, 0, 0, 0, 0]), false).unwrap();
        let data = prepared.data.unwrap();

        assert_eq!(&data[4..8], &[20, 40, 80, 0]);
    }

    #[test]
    fn premultiplies_only_the_egui_derivative() {
        let source = image(1, 1, vec![200, 100, 50, 128]);
        let straight = prepare_image(&source, false).unwrap().data.unwrap();
        let premultiplied = prepare_image(&source, true).unwrap().data.unwrap();

        assert_eq!(straight, vec![200, 100, 50, 128]);
        assert_eq!(premultiplied, vec![100, 50, 25, 128]);
        assert_eq!(source.data.unwrap(), vec![200, 100, 50, 128]);
    }

    #[test]
    fn egui_icons_use_a_crisp_nearest_mip_with_linear_texel_filtering() {
        let source = image(4, 4, vec![255; 4 * 4 * 4]);
        let scene_image = prepare_image(&source, false).unwrap();
        let egui_icon = prepare_image(&source, true).unwrap();

        assert_eq!(scene_image.sampler, ImageSampler::linear());
        let ImageSampler::Descriptor(icon_sampler) = egui_icon.sampler else {
            panic!("egui icon should have its own sampler");
        };
        assert_eq!(icon_sampler.min_filter, ImageFilterMode::Linear);
        assert_eq!(icon_sampler.mag_filter, ImageFilterMode::Linear);
        assert_eq!(icon_sampler.mipmap_filter, ImageFilterMode::Nearest);
    }
}
