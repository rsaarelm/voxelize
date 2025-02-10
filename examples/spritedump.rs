use anyhow::Result;
use image::{ImageBuffer, Rgba};

// Render an oblique sprite from a VOX model.

pub type Pixel = Rgba<u8>;
pub type Image = ImageBuffer<Pixel, Vec<u8>>;

fn main() -> Result<()> {
    // Load VOX model from CLI parameter
    let path = std::env::args().nth(1).expect("No file path provided");

    let scene = dot_vox::load(&path).expect("Failed to load");
    let model = &scene.models[0];

    // Get dimensions of model and build a blank image that's x+z, y+z big.

    let mut canvas = Image::new(
        model.size.x as u32 + model.size.z as u32,
        model.size.y as u32 + model.size.z as u32,
    );

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
                let y = model.size.y - y - 1;
                let z = model.size.z - z - 1;
                let x = x + z / 2;
                let y = y + z / 2;
                canvas.put_pixel(x, y, Rgba([color.r, color.g, color.b, 255]));
            }
        }
    }

    canvas.save("output.png")?;

    Ok(())
}
