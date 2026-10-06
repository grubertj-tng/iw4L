//! STL triangle soup, binary or ASCII. Facet normals are not read: the import
//! derives its own from the winding.

use std::path::Path;

pub type Triangle = [[f32; 3]; 3];

pub fn load(path: &Path) -> Result<Vec<Triangle>, String> {
    let bytes =
        std::fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    parse(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn parse(bytes: &[u8]) -> Result<Vec<Triangle>, String> {
    // Binary files may also start with "solid", so the size is what decides.
    if let Some(count) = binary_triangle_count(bytes) {
        return Ok(bytes[84..84 + count * 50]
            .as_chunks::<50>()
            .0
            .iter()
            .map(|facet| {
                let f = |i: usize| {
                    f32::from_le_bytes(facet[12 + 4 * i..16 + 4 * i].try_into().unwrap())
                };
                [[f(0), f(1), f(2)], [f(3), f(4), f(5)], [f(6), f(7), f(8)]]
            })
            .collect());
    }
    match std::str::from_utf8(bytes) {
        Ok(text) if text.trim_start().starts_with("solid") && text.contains("endsolid") => {
            parse_ascii(text)
        }
        _ => Err("neither a binary STL of consistent size nor ASCII STL".to_owned()),
    }
}

fn binary_triangle_count(bytes: &[u8]) -> Option<usize> {
    let count = u32::from_le_bytes(bytes.get(80..84)?.try_into().ok()?) as usize;
    (bytes.len() == 84 + count.checked_mul(50)?).then_some(count)
}

fn parse_ascii(text: &str) -> Result<Vec<Triangle>, String> {
    let mut triangles = Vec::new();
    let mut corners = Vec::with_capacity(3);
    let mut words = text.split_whitespace();
    while let Some(word) = words.next() {
        if word != "vertex" {
            continue;
        }
        let mut corner = [0.0f32; 3];
        for axis in &mut corner {
            let word = words.next().ok_or("ASCII STL ends inside a vertex")?;
            *axis = word
                .parse()
                .map_err(|_| format!("ASCII STL vertex has \"{word}\""))?;
        }
        corners.push(corner);
        if corners.len() == 3 {
            triangles.push([corners[0], corners[1], corners[2]]);
            corners.clear();
        }
    }
    if !corners.is_empty() {
        return Err("ASCII STL has a facet with fewer than three vertices".to_owned());
    }
    Ok(triangles)
}
