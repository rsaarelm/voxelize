use std::collections::HashSet;

use anyhow::Result;
use dot_vox::DotVoxData;
use image::{GenericImageView, ImageBuffer, Rgba};

// Render an oblique sprite from a VOX model.

pub type Pixel = Rgba<u8>;
pub type Image = ImageBuffer<Pixel, Vec<u8>>;

fn main() -> Result<()> {
    // Load VOX model from CLI parameter
    let path = std::env::args().nth(1).expect("No file path provided");

    let scene = dot_vox::load(&path).expect("Failed to load");
    let model = &scene.models[0];

    // Get dimensions of model and build a blank image that's x+z, y+z big.

    let w = model.size.x as u32 + model.size.z as u32;
    let h = model.size.y as u32 + model.size.z as u32;

    let mut canvas = Image::new(w * 2, h * 2);

    draw_model(&scene, &mut canvas, (0, 0), false);
    draw_model(&scene, &mut canvas, (0, h), true);

    // Comparison image 1
    if let Some(front_path) = std::env::args().nth(2) {
        let image = image::open(front_path).expect("Failed to load front image");
        // Draw front_image pixels on canvas
        for (x, y, pixel) in image.pixels() {
            canvas.put_pixel(x + w, y, pixel);
        }
    }

    // Comparison image 2
    if let Some(rear_path) = std::env::args().nth(3) {
        let image = image::open(rear_path).expect("Failed to load rear image");
        // Draw front_image pixels on canvas
        for (x, y, pixel) in image.pixels() {
            canvas.put_pixel(x + w, y + h, pixel);
        }
    }

    canvas.save("output.png")?;

    Ok(())
}

fn draw_model(scene: &DotVoxData, canvas: &mut Image, position: (u32, u32), flip: bool) {
    let model = &scene.models[0];
    let mut filled = HashSet::new();
    for z in 0..model.size.z {
        for y in 0..model.size.y {
            for x in 0..model.size.x {
                let Some(voxel) = model
                    .voxels
                    .iter()
                    .find(|v| v.x == x as u8 && v.y == y as u8 && v.z == z as u8)
                else {
                    continue;
                };
                let color = scene.palette[voxel.i as usize];
                let y = if flip { y } else { model.size.y - y - 1 };
                let z = model.size.z - z - 1;
                let x = x + z / 2;
                let y = y + z / 2;
                let (x, y) = (x + position.0, y + position.1);
                canvas.put_pixel(x, y, Rgba([color.r, color.g, color.b, 255]));
                filled.insert((x, y));
            }
        }
    }

    // Draw black outline around the drawn model.
    for &(x, y) in &filled {
        for dx in -1i32..=1 {
            for dy in -1i32..=1 {
                if dx.abs() + dy.abs() == 1 {
                    let x = (x as i32 + dx) as u32;
                    let y = (y as i32 + dy) as u32;
                    if !filled.contains(&(x, y)) {
                        canvas.put_pixel(x, y, Rgba([0, 0, 0, 255]));
                    }
                }
            }
        }
    }
}
