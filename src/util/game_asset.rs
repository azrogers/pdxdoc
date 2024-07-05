use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

use anyhow::Result;
use ddsfile::Dds;
use image::{
    imageops::FilterType, load_from_memory, load_from_memory_with_format, DynamicImage,
    GenericImageView,
};
use image_dds::image_from_dds;

#[derive(Clone)]
pub enum AssetSizeMode {
    None,
    /// The maximum width or height of the asset requested.
    /// It will be scaled down if either is greater than this number, preserving aspect ratio.
    MaxDimension(u32),
}

#[derive(Clone)]
pub struct RequestedAsset {
    pub target_url: String,
    pub source: PathBuf,
    pub size_mode: AssetSizeMode,
}

pub struct GameAssets;

impl GameAssets {
    pub fn new_filename_for_asset(orig: &Path) -> Option<PathBuf> {
        let new_ext = match orig.extension()?.to_str()? {
            "dds" => "png",
            v => v,
        };

        Some(orig.with_extension(new_ext))
    }

    pub fn convert_image(asset: &RequestedAsset, output_path: &Path) -> Result<()> {
        let mut f = fs::read(&asset.source)?;
        let dds = Dds::read(&f[..])?;

        let mut image = DynamicImage::ImageRgba8(image_from_dds(&dds, 0)?);
        let mut out = File::open(&output_path)?;
        let (width, height) = image.dimensions();
        match asset.size_mode {
            AssetSizeMode::MaxDimension(dim) if width > dim || height > dim => {
                image = image.resize(dim, dim, FilterType::Lanczos3);
            }
            _ => {}
        }

        image.write_to(&mut out, image::ImageFormat::Png)?;
        Ok(())
    }
}
