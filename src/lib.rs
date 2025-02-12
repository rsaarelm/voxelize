use std::collections::HashMap;

use dot_vox::DotVoxData;
use glam::{ivec2, ivec3, vec2, vec3, IVec2, IVec3, Mat4, Vec2, Vec3};
use image::{ImageBuffer, Rgba};

pub type Pixel = Rgba<u8>;
pub type Image = ImageBuffer<Pixel, Vec<u8>>;

/// Trace voxel cells a ray will pass through.
pub fn trace(origin: Vec3, dir: Vec3) -> impl Iterator<Item = Vec3> {
    // DDA algorithm.

    // XXX: This misses some side voxels but should always draw a solid line.
    assert!(
        dir.length_squared() > 0.00001,
        "Direction vector must be non-zero"
    );

    // Normalize longest axis to unit length.
    let scale = 1.0 / dir.abs().max_element();
    let dir = scale * dir;

    let mut pos = origin;
    std::iter::from_fn(move || {
        let cell = pos.round();
        pos += dir;
        Some(cell)
    })
}

/// A volumetric object of some sort.
pub trait Body {
    type Value;

    fn sample(&self, pos: Vec3) -> Option<Self::Value>;

    fn bounding_box(&self) -> BoundingBox {
        Default::default()
    }

    fn normal(&self, pos: Vec3) -> Vec3 {
        let mut n = Vec3::ZERO;
        for x in -1..=1i32 {
            for y in -1..=1i32 {
                for z in -1..=1i32 {
                    if x.abs() + y.abs() + z.abs() != 1 {
                        continue;
                    }
                    let d = ivec3(x, y, z).as_vec3();
                    if self.sample(pos + d).is_none() {
                        n += d;
                    }
                }
            }
        }

        if n.length_squared() > 0.0 {
            n.normalize()
        } else {
            Vec3::ZERO
        }
    }
}

impl Body for dot_vox::Model {
    type Value = u8;

    fn sample(&self, pos: Vec3) -> Option<Self::Value> {
        // Check bounds.
        if pos.min_element() < 0.0 {
            return None;
        }

        if pos.x as u32 >= self.size.x || pos.y as u32 >= self.size.y || pos.z as u32 >= self.size.z
        {
            return None;
        }

        let (x, y, z) = (pos.x as u8, pos.y as u8, pos.z as u8);

        self.voxels
            .iter()
            .find_map(|voxel| (voxel.x == x && voxel.y == y && voxel.z == z).then_some(voxel.i))
    }

    fn bounding_box(&self) -> BoundingBox {
        let (min, max) =
            self.voxels
                .iter()
                .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), voxel| {
                    let voxel = vec3(voxel.x as f32, voxel.y as f32, voxel.z as f32);
                    (Vec3::min(min, voxel), Vec3::max(max, voxel))
                });

        BoundingBox::new(min, max)
    }
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
        const OB: f32 = 0.5;

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

#[derive(Copy, Clone, Default, Debug)]
pub struct BoundingBox {
    pub min: Vec3,
    pub max: Vec3,
}

impl BoundingBox {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    pub fn contains(&self, pos: Vec3) -> bool {
        self.min.cmple(pos).all() && pos.cmplt(self.max).all()
    }

    /// List the eight corners of the box.
    pub fn corners(&self) -> impl Iterator<Item = Vec3> + '_ {
        let dim = self.max - self.min;

        (0..8).map(move |bits| {
            // Enumerate the corners using three bits.
            self.min
                + dim
                    * vec3(
                        (bits & 1) as f32,
                        ((bits >> 1) & 1) as f32,
                        ((bits >> 2) & 1) as f32,
                    )
        })
    }

    /// Return origin and size of the screen space bounding box.
    pub fn screen_bounds(&self, camera: &Mat4) -> (IVec2, IVec2) {
        let mut screen_min = Vec3::splat(f32::INFINITY);
        let mut screen_max = Vec3::splat(f32::NEG_INFINITY);
        for p in self.corners() {
            let p = camera.transform_point3(p);
            screen_min = screen_min.min(p);
            screen_max = screen_max.max(p);
        }
        let origin = screen_min.truncate().floor().as_ivec2();
        let size = (screen_max - screen_min).truncate().ceil().as_ivec2();

        (origin, size)
    }
}

pub fn build_view<T>(model: &dyn Body<Value = T>, camera: &Mat4) -> HashMap<IVec2, (Vec3, T)> {
    // How far to raytrace until you bail out.
    const TRACE_LIMIT: usize = 256;

    let aabb = model.bounding_box();

    let (origin, size) = aabb.screen_bounds(camera);

    let mut ret = HashMap::default();

    for y in 0..size.y {
        for x in 0..size.x {
            let view_pos = ivec2(x, y);

            // Flip y-axis when moving from image space to 3D space.
            let (x, y) = (x as f32, size.y as f32 - y as f32 - 1.0);
            // Ray pointing towards scene at negative z.
            let pos = vec3(x + origin.x as f32, y + origin.y as f32, 0.0);
            let dir = vec3(0.0, 0.0, -1.0);

            let pos = camera.inverse().transform_point3(pos);
            let dir = camera.inverse().transform_vector3(dir);

            if let Some(result) = trace(pos, dir)
                .take(TRACE_LIMIT)
                .find_map(|cell| model.sample(cell).map(|val| (cell, val)))
            {
                ret.insert(view_pos, result);
            }
        }
    }

    ret
}

/// Remove black outline from the image.
pub fn clear_outline(image: &mut Image) {
    let color_key = *image.get_pixel(0, 0);
    let outline_color = Rgba([0, 0, 0, 255]);

    let safe_get = |image: &Image, x: i32, y: i32| -> Pixel {
        if x < 0 || y < 0 {
            return color_key;
        }
        image
            .get_pixel_checked(x as u32, y as u32)
            .copied()
            .unwrap_or(color_key)
    };

    for y in 0..(image.height() as i32) {
        for x in 0..(image.width() as i32) {
            // Remove pixels of outline color if they're adjacent to image
            // edge.
            if safe_get(image, x, y) == outline_color
                && (safe_get(image, x - 1, y) == color_key
                    || safe_get(image, x + 1, y) == color_key
                    || safe_get(image, x, y - 1) == color_key
                    || safe_get(image, x, y + 1) == color_key)
            {
                image.put_pixel(x as u32, y as u32, color_key);
            }
        }
    }
}

pub struct Rect {
    pub min: IVec2,
    pub max: IVec2,
}

impl Rect {
    pub fn new(min: IVec2, max: IVec2) -> Self {
        Self { min, max }
    }

    pub fn from_image(image: &Image) -> Self {
        let color_key = *image.get_pixel(0, 0);
        Self::from_points(
            image
                .enumerate_pixels()
                .filter(|&(_, _, &pixel)| pixel != color_key)
                .map(|(x, y, _)| ivec2(x as i32, y as i32)),
        )
    }

    pub fn from_points(points: impl Iterator<Item = IVec2>) -> Self {
        let (min, max) = points.fold((IVec2::MAX, IVec2::MIN), |(min, max), p| {
            (min.min(p), max.max(p))
        });

        Self::new(min, max + ivec2(1, 1))
    }

    /// Map point within the rectangle to [0, 1[ range.
    pub fn normalize(&self, pos: IVec2) -> Vec2 {
        let size = self.max - self.min;
        let pos = pos - self.min;
        vec2(pos.x as f32 / size.x as f32, pos.y as f32 / size.y as f32)
    }

    /// Map point in [0, 1[ range to rectangle.
    pub fn denormalize(&self, uv: Vec2) -> IVec2 {
        let size = self.max - self.min;
        let pos = vec2(uv.x * size.x as f32, uv.y * size.y as f32);
        self.min + pos.floor().as_ivec2()
    }
}

pub trait DotVoxExt {
    fn set_voxel(&mut self, model_idx: usize, pos: IVec3, color: Pixel);
}

impl DotVoxExt for DotVoxData {
    fn set_voxel(&mut self, model_idx: usize, pos: IVec3, color: Pixel) {
        // Find the palette color that is closest to the color we want.
        let palette_idx = self
            .palette
            .iter()
            .enumerate()
            .min_by_key(|(_, &p)| {
                (p.r as i32 - color[0] as i32).abs()
                    + (p.g as i32 - color[1] as i32).abs()
                    + (p.b as i32 - color[2] as i32).abs()
            })
            .map(|(idx, _)| idx)
            .unwrap();

        let (x, y, z) = (pos.x as u8, pos.y as u8, pos.z as u8);

        let voxel_idx = self.models[model_idx]
            .voxels
            .iter()
            .position(|v| v.x == x && v.y == y && v.z == z)
            .unwrap_or_else(|| {
                self.models[model_idx].voxels.push(dot_vox::Voxel {
                    x,
                    y,
                    z,
                    i: palette_idx as u8,
                });
                self.models[model_idx].voxels.len() - 1
            });

        self.models[model_idx].voxels[voxel_idx].i = palette_idx as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_normalization() {
        let rect = Rect::new(ivec2(10, 20), ivec2(30, 40));
        assert_eq!(rect.normalize(ivec2(10, 20)), vec2(0.0, 0.0));
        assert_eq!(rect.normalize(ivec2(30, 40)), vec2(1.0, 1.0));

        assert_eq!(rect.denormalize(vec2(0.0, 0.0)), ivec2(10, 20));
        assert_eq!(rect.denormalize(vec2(1.0, 1.0)), ivec2(30, 40));
    }
}
