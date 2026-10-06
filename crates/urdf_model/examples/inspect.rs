//! Prints what the import makes of a URDF: `cargo run --release -p urdf_model
//! --example inspect -- path/to/robot.urdf`.

fn main() -> Result<(), String> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: inspect <robot.urdf>")?;
    let started = std::time::Instant::now();
    let robot = urdf_model::Robot::load(std::path::Path::new(&path))?;
    let model = urdf_model::RobotModel::build(&robot, urdf_model::Detail::default())?;
    let elapsed = started.elapsed();
    let skel = model.skel(&model.name, |_| {
        asset_core::WalkLocalMaterialIndex::from_walk(0)
    });
    let (lo, hi) = skel.bounds.ok_or("the model has no vertices")?;
    println!(
        "{}: {} bones, {} parts, {} triangles from {}, {} vertices, {} surfaces, {:.1}x{:.1}x{:.1} in, radius {:.1} in, {:.0} ms",
        model.name,
        model.bones.len(),
        model.parts.len(),
        model.triangle_count(),
        model.source_triangles,
        model.vertex_count(),
        skel.surface_vertex_ranges.len(),
        hi[0] - lo[0],
        hi[1] - lo[1],
        hi[2] - lo[2],
        skel.radius.unwrap_or(0.0),
        elapsed.as_secs_f64() * 1000.0,
    );
    for part in &model.parts {
        println!(
            "  {:28} {:6} tris {:6} verts rgba {:?}",
            model.bones[part.bone].name,
            part.mesh.triangle_count(),
            part.mesh.positions.len(),
            part.rgba
        );
    }
    Ok(())
}
