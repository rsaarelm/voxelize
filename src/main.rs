use std::{fs::File, path::PathBuf};

use anyhow::{anyhow, Result};
use clap::{Args, Parser, Subcommand};
use glam::{vec3, IVec2, Mat4, Vec3};
use voxelize::{Camera, DotVoxExt, Image, Rect};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Dump the projected image from a voxel model.
    Dump(DumpArgs),

    /// Paint the surface of a voxel model using a reference image.
    Paint(PaintArgs),
}

#[derive(Args, Debug)]
struct PaintArgs {
    /// Sample sprite is viewed from the front.
    #[arg(long, default_value = "true")]
    front: bool,

    /// Sample sprite is viewed from the back.
    #[arg(long, conflicts_with = "front")]
    back: bool,

    /// Sample sprite to paint on model.
    #[arg(long)]
    src: String,

    /// The VOX model to paint.
    model: String,
}

#[derive(Args, Debug)]
struct DumpArgs {
    /// The VOX model to dump.
    model: String,

    /// How big should the output image be.
    #[arg(long, default_value = "1.0")]
    scale: f32,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Dump(args) => dump(args.scale, &args.model)?,
        Command::Paint(args) => {
            let camera = if args.back {
                Camera::ObliqueSouth
            } else {
                // Default is front
                Camera::ObliqueNorth
            };
            paint(&args.model, camera, &args.src)?
        }
    }
    Ok(())
}

fn dump(scale: f32, model: &str) -> Result<()> {
    let output_name = PathBuf::from(model).with_extension("png");

    let scene = dot_vox::load(model).map_err(|e| anyhow!(e))?;

    let camera = Mat4::from(Camera::ObliqueNorth);
    // Pull camera backwards to see the model.
    // Scale according to scale param.
    let camera = Mat4::from_scale(Vec3::splat(scale))
        * Mat4::from_translation(vec3(0.0, 0.0, -50.0))
        * camera;

    let view = voxelize::build_view(&scene.models[0], &camera);

    let (p1, p2) = view
        .keys()
        .fold((IVec2::MAX, IVec2::MIN), |(min, max), &pos| {
            (min.min(pos), max.max(pos))
        });

    // Size of the border to put around the image in pixels.
    const BORDER: u32 = 1;

    let mut canvas = Image::new(
        (p2.x - p1.x) as u32 + 1 + BORDER * 2,
        (p2.y - p1.y) as u32 + 1 + BORDER * 2,
    );
    for (pos, (_, idx)) in &view {
        let color = scene.palette[*idx as usize];
        let color = image::Rgba([color.r, color.g, color.b, 255]);
        let pos = *pos - p1;
        canvas.put_pixel(pos.x as u32 + BORDER, pos.y as u32 + BORDER, color);
    }

    canvas.save(output_name)?;

    Ok(())
}

fn paint(model_path: &str, camera: Camera, src: &str) -> Result<()> {
    let mut scene = dot_vox::load(model_path).map_err(|e| anyhow!(e))?;

    let mut src: Image = image::open(src)?.into();
    let color_key = *src.get_pixel(0, 0);
    // Clear black outline from source.
    voxelize::clear_outline(&mut src);

    let src_bounds = Rect::from_image(&src);

    let camera = Mat4::from(camera);

    // Pull camera back, scale up the model so we hit all voxels.
    let camera =
        Mat4::from_scale(Vec3::splat(2.0)) * Mat4::from_translation(vec3(0.0, 0.0, -50.0)) * camera;

    let view = voxelize::build_view(&scene.models[0], &camera);

    let view_bounds = Rect::from_points(view.keys().copied());

    for (pos, (vox_pos, _)) in &view {
        let vox_pos = vox_pos.as_ivec3();
        // Convert between bounding boxes to get the source point.
        let src_pos = src_bounds.denormalize(view_bounds.normalize(*pos));
        let color = *src.get_pixel(src_pos.x as u32, src_pos.y as u32);
        if color == color_key {
            continue;
        }
        scene.set_voxel(0, vox_pos, color);
    }

    scene.write_vox(&mut File::create(model_path)?)?;

    Ok(())
}
