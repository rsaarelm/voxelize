use std::collections::HashMap;

use anyhow::bail;
use glam::{ivec2, ivec3, vec3, IVec2, IVec3, Mat4, Vec3};
use image::{ImageBuffer, Rgba};
use itertools::Itertools;

pub type Pixel = Rgba<u8>;
pub type Image = ImageBuffer<Pixel, Vec<u8>>;

pub struct FocusImage {
    image: Image,
    center: IVec2,
}

impl TryFrom<Image> for FocusImage {
    type Error = anyhow::Error;

    fn try_from(image: Image) -> Result<Self, Self::Error> {
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
                    Rgba([0, 0, 0, 0])
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
        let y = uv.y.round() as i32 + self.center.y;
        if x > 0 && y > 0 && x < self.image.width() as i32 && y < self.image.height() as i32 {
            let p = *self.image.get_pixel(x as u32, y as u32);
            (p != Rgba([0, 0, 0, 0])).then_some(p)
        } else {
            None
        }
    }
}

pub struct Prism {
    image: FocusImage,
    camera: Mat4,
}

impl Prism {
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

pub fn build_model(views: &[Prism]) -> HashMap<IVec3, Vec<VoxelMatch>> {
    let mut ret: HashMap<IVec3, Vec<VoxelMatch>> = Default::default();

    // TODO: Define bounding box size from views.
    const N: i32 = 16;
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
                    } else {
                        // If any of the views fails to match the position,
                        // don't add it to the model.
                        continue;
                    }
                }

                if !matches.is_empty() {
                    ret.insert(ivec3(x, y, z), matches);
                }
            }
        }
    }

    ret
}

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
        const TURN: f32 = std::f32::consts::FRAC_PI_4;
        // Oblique unit.
        const OB: f32 = std::f32::consts::FRAC_1_SQRT_2;

        match value {
            // Rotate for facing.
            ObliqueNorth => {}
            ObliqueWest => ret *= Mat4::from_rotation_z(TURN),
            ObliqueSouth => ret *= Mat4::from_rotation_z(2.0 * TURN),
            ObliqueEast => ret *= Mat4::from_rotation_z(3.0 * TURN),
        }

        match value {
            // Apply oblique shear.
            ObliqueNorth | ObliqueEast | ObliqueSouth | ObliqueWest => {
                ret.z_axis.x = -OB;
                ret.z_axis.y = OB;
            }
        }

        ret
    }
}

fn main() {
    println!("Hello, world!");
}
