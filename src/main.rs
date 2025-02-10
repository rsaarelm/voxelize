use std::{cmp, collections::HashMap};

use anyhow::{bail, Result};
use glam::{ivec2, ivec3, vec3, IVec2, IVec3, Mat4, Vec3};
use image::{ImageBuffer, Rgba};
use itertools::Itertools;
use vox_format::{
    types::{Color, Model, Point, Size, Voxel},
    VoxData,
};

pub type Pixel = Rgba<u8>;
pub type Image = ImageBuffer<Pixel, Vec<u8>>;

/// Transparent color in images we generate.
pub const DEFAULT_KEY: Pixel = Rgba([0, 0, 0, 0]);

pub struct FocusImage {
    image: Image,
    center: IVec2,
}

impl TryFrom<Image> for FocusImage {
    type Error = anyhow::Error;

    fn try_from(image: Image) -> Result<Self> {
        // The source for a focus image must have the lines at x=0 and y=0 be
        // fully transparent except for a single pixel. The positions of these
        // pixels will be used to determine the x and y of the center point.
        // If the pixels aren't found or there is more than one
        // non-transparent pixel on either line, the construction will fail.

        if image.width() < 2 || image.height() < 2 {
            bail!("Invalid image size");
        }

        // Transparent color.
        let key = *image.get_pixel(0, 0);

        let Ok(Some(x)) = (0..image.width())
            .filter(|&x| *image.get_pixel(x, 0) != key)
            .at_most_one()
        else {
            bail!("No unique x-focus pixel found");
        };

        let Ok(Some(y)) = (0..image.height())
            .filter(|&y| *image.get_pixel(0, y) != key)
            .at_most_one()
        else {
            bail!("No unique y-focus pixel found");
        };

        // Construct a new image with the lines at x=0 and y=0 cut off.
        Ok(FocusImage {
            image: Image::from_fn(image.width() - 1, image.height() - 1, |x, y| {
                let p = *image.get_pixel(x + 1, y + 1);
                if p == key {
                    DEFAULT_KEY
                } else {
                    p
                }
            }),
            center: ivec2(x as i32, y as i32),
        })
    }
}

impl FocusImage {
    pub fn sample(&self, uv: Vec3) -> Option<Pixel> {
        let x = uv.x.round() as i32 + self.center.x;
        let y = -uv.y.round() as i32 + self.center.y;
        if x > 0 && y > 0 && x < self.image.width() as i32 && y < self.image.height() as i32 {
            let p = *self.image.get_pixel(x as u32, y as u32);
            (p != DEFAULT_KEY).then_some(p)
        } else {
            None
        }
    }

    fn safe_get(&self, x: i32, y: i32) -> Pixel {
        if x < 0 || y < 0 {
            return DEFAULT_KEY;
        }

        self.image
            .get_pixel_checked(x as u32, y as u32)
            .copied()
            .unwrap_or(DEFAULT_KEY)
    }

    pub fn remove_outline(&mut self, outline_color: Pixel) {
        for y in 0..(self.image.height() as i32) {
            for x in 0..(self.image.width() as i32) {
                // Remove pixels of outline color if they're adjacent to image
                // edge.
                if self.safe_get(x, y) == outline_color
                    && (self.safe_get(x - 1, y) == DEFAULT_KEY
                        || self.safe_get(x + 1, y) == DEFAULT_KEY
                        || self.safe_get(x, y - 1) == DEFAULT_KEY
                        || self.safe_get(x, y + 1) == DEFAULT_KEY)
                {
                    self.image.put_pixel(x as u32, y as u32, DEFAULT_KEY);
                }
            }
        }
    }
}

pub struct Prism {
    image: FocusImage,
    camera: Mat4,
}

impl Prism {
    pub fn new(image: FocusImage, camera: impl Into<Mat4>) -> Result<Self> {
        Ok(Prism {
            image,
            camera: camera.into(),
        })
    }

    pub fn sample(&self, pos: Vec3) -> Option<Pixel> {
        self.image.sample(self.camera.transform_point3(pos))
    }

    pub fn normal(&self) -> Vec3 {
        self.camera.transform_vector3(Vec3::Z)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct VoxelMatch {
    pub color: Pixel,
    pub normal: Vec3,
}

pub fn build_model(views: &[Prism]) -> HashMap<IVec3, Pixel> {
    assert!(!views.is_empty());

    let mut hits: HashMap<IVec3, Vec<VoxelMatch>> = Default::default();

    // TODO: Define bounding box size from views.
    const N: i32 = 64;
    for z in -N..=N {
        for y in -N..=N {
            for x in -N..=N {
                let pos = vec3(x as f32, y as f32, z as f32);

                let mut matches = Vec::new();
                for view in views {
                    if let Some(color) = view.sample(pos) {
                        matches.push(VoxelMatch {
                            color,
                            normal: view.normal(),
                        });
                    }
                }

                if matches.len() == views.len() {
                    hits.insert(ivec3(x, y, z), matches);
                }
            }
        }
    }

    // Clean up for actual result.
    let mut ret = HashMap::new();
    for (pos, matches) in &hits {
        let mut exposed_faces = Vec::new();
        for x in -1i32..=1 {
            for y in -1i32..=1 {
                for z in -1i32..=1 {
                    if x.abs() + y.abs() + z.abs() != 1 {
                        continue;
                    }

                    let d = ivec3(x, y, z);
                    if !hits.contains_key(&(pos + d)) {
                        exposed_faces.push(d.as_vec3());
                    }
                }
            }
        }

        // Crude voxel surface normal based on open faces.
        let normal = exposed_faces
            .iter()
            .fold(Vec3::ZERO, |a, b| a + b)
            .normalize();

        // Find the match whose normal is closest to the surface normal.
        let color: Pixel = matches
            .iter()
            .min_by_key(|m| {
                let diff = m.normal.dot(normal);
                // Reverse the order so the best match is the one with the highest
                // dot product.
                //
                // HACK: Bits gets us an ordering-preserving Ord-able value from
                // f32.
                cmp::Reverse(diff.to_bits())
            })
            .map(|m| m.color)
            .unwrap();

        ret.insert(*pos, color);
    }

    ret
}

pub fn to_vox(model: &HashMap<IVec3, Pixel>) -> VoxData {
    let mut palette = Vec::new();
    let mut ret = VoxData::default();

    let min: IVec3 = model.keys().copied().reduce(|a, b| a.min(b)).unwrap();
    let max: IVec3 = model.keys().copied().reduce(|a, b| a.max(b)).unwrap();

    let size = max - min + ivec3(1, 1, 1);
    let mut vox_model = Model {
        size: Size {
            x: size.x as u32,
            y: size.y as u32,
            z: size.z as u32,
        },
        voxels: Vec::new(),
    };

    for (pos, color) in model {
        let color = Color::new(color[0], color[1], color[2], color[3]);
        // Try to find color already in palette and use its index. If it's not
        // there, add it to the end and use the new index.
        let color_index = palette.iter().position(|&c| c == color).unwrap_or_else(|| {
            palette.push(color);
            palette.len() - 1
        }) as u8;

        let pos = pos - min;
        vox_model.voxels.push(Voxel {
            point: Point {
                x: pos.x as i8,
                y: pos.y as i8,
                z: pos.z as i8,
            },
            color_index: color_index.into(),
        });
    }

    ret.models.push(vox_model);
    for (i, &c) in palette.iter().enumerate() {
        assert!(i < 256);
        ret.palette.colors[i] = c;
    }

    ret
}

#[derive(Copy, Clone, Debug)]
pub enum Camera {
    ObliqueNorth,
    ObliqueEast,
    ObliqueSouth,
    ObliqueWest,
}

use Camera::*;

impl From<Camera> for Mat4 {
    fn from(value: Camera) -> Self {
        let mut ret = Mat4::IDENTITY;
        // 90 degree turn.
        const TURN: f32 = std::f32::consts::FRAC_PI_2;
        // Oblique unit.
        const OB: f32 = std::f32::consts::FRAC_1_SQRT_2;
        //const OB: f32 = 0.5;

        match value {
            // Apply oblique shear.
            ObliqueNorth | ObliqueEast | ObliqueSouth | ObliqueWest => {
                ret.z_axis.x = -OB;
                ret.z_axis.y = OB;
            }
        }

        match value {
            // Rotate for facing.
            ObliqueNorth => {}
            ObliqueEast => ret *= Mat4::from_rotation_z(TURN),
            ObliqueSouth => ret *= Mat4::from_rotation_z(2.0 * TURN),
            ObliqueWest => ret *= Mat4::from_rotation_z(3.0 * TURN),
        }

        ret
    }
}

fn main() -> Result<()> {
    let mut views = Vec::new();
    let sprite = "noble";

    for (i, camera) in [ObliqueNorth, ObliqueWest, ObliqueSouth, ObliqueEast]
        .iter()
        .enumerate()
    {
        let image = Image::from(image::open(format!("sprites/{sprite}{i}.png"))?);
        let mut image = FocusImage::try_from(image)?;
        image.remove_outline(Rgba([0, 0, 0, 255]));
        views.push(Prism::new(image, *camera)?);
    }

    let model = build_model(&views);

    eprintln!("Model size: {}", model.len());

    let vox = to_vox(&model);
    vox_format::to_file("output.vox", &vox).unwrap();

    Ok(())
}
