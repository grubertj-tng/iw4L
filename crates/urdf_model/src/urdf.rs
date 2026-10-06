//! The subset of URDF a rigid visual import needs: links with mesh visuals,
//! their material colours, and the joint tree that places them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::math::{DMat3, DVec3};

/// A rigid transform in the URDF's own frame and units (metres).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub rotation: DMat3,
    pub translation: DVec3,
}

impl Pose {
    pub const IDENTITY: Self = Self {
        rotation: DMat3::IDENTITY,
        translation: DVec3::ZERO,
    };

    pub fn then(self, child: Self) -> Self {
        Self {
            rotation: self.rotation * child.rotation,
            translation: self.translation + self.rotation * child.translation,
        }
    }

    pub fn apply(self, point: DVec3) -> DVec3 {
        self.translation + self.rotation * point
    }
}

#[derive(Clone, Debug)]
pub struct Visual {
    pub origin: Pose,
    pub mesh: PathBuf,
    pub scale: DVec3,
    pub rgba: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct Link {
    pub name: String,
    pub visuals: Vec<Visual>,
}

#[derive(Clone, Debug)]
pub struct Joint {
    pub name: String,
    pub parent: usize,
    pub child: usize,
    pub origin: Pose,
}

#[derive(Clone, Debug)]
pub struct Robot {
    pub name: String,
    pub links: Vec<Link>,
    pub joints: Vec<Joint>,
}

/// URDF leaves an uncoloured visual to the viewer; this is the grey most
/// viewers draw it with.
const UNCOLOURED: [f32; 4] = [0.7, 0.7, 0.7, 1.0];

impl Robot {
    pub fn load(urdf: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(urdf)
            .map_err(|error| format!("cannot read {}: {error}", urdf.display()))?;
        Self::parse(&text, urdf.parent().unwrap_or(Path::new(".")))
            .map_err(|error| format!("{}: {error}", urdf.display()))
    }

    pub fn parse(text: &str, base: &Path) -> Result<Self, String> {
        let document = roxmltree::Document::parse(text).map_err(|error| error.to_string())?;
        let root = document.root_element();
        if root.tag_name().name() != "robot" {
            return Err(format!(
                "root element is <{}>, not <robot>",
                root.tag_name().name()
            ));
        }
        let mut colours = HashMap::new();
        for material in children(root, "material") {
            if let (Some(name), Some(rgba)) = (material.attribute("name"), colour_of(material)?) {
                colours.insert(name.to_owned(), rgba);
            }
        }

        let mut links = Vec::new();
        let mut link_index = HashMap::new();
        for link in children(root, "link") {
            let name = link.attribute("name").ok_or("a <link> has no name")?;
            let mut visuals = Vec::new();
            for visual in children(link, "visual") {
                let Some(mesh) = children(visual, "geometry")
                    .next()
                    .and_then(|geometry| children(geometry, "mesh").next())
                else {
                    // Primitive shapes carry no authored surface; the imported
                    // model is the mesh shell.
                    continue;
                };
                let filename = mesh
                    .attribute("filename")
                    .ok_or_else(|| format!("link {name}: <mesh> has no filename"))?;
                let scale = match mesh.attribute("scale") {
                    Some(scale) => vec3(scale).map_err(|error| format!("link {name}: {error}"))?,
                    None => DVec3::ONE,
                };
                let rgba = match children(visual, "material").next() {
                    Some(material) => match colour_of(material)? {
                        Some(rgba) => rgba,
                        None => material
                            .attribute("name")
                            .and_then(|key| colours.get(key).copied())
                            .unwrap_or(UNCOLOURED),
                    },
                    None => UNCOLOURED,
                };
                visuals.push(Visual {
                    origin: origin_of(visual).map_err(|error| format!("link {name}: {error}"))?,
                    mesh: resolve_mesh(base, filename)?,
                    scale,
                    rgba,
                });
            }
            if link_index.insert(name.to_owned(), links.len()).is_some() {
                return Err(format!("link {name} is declared twice"));
            }
            links.push(Link {
                name: name.to_owned(),
                visuals,
            });
        }

        let mut joints = Vec::new();
        for joint in children(root, "joint") {
            let name = joint.attribute("name").ok_or("a <joint> has no name")?;
            let link = |tag: &'static str| -> Result<usize, String> {
                let target = children(joint, tag)
                    .next()
                    .and_then(|node| node.attribute("link"))
                    .ok_or_else(|| format!("joint {name} has no <{tag} link=…>"))?;
                link_index
                    .get(target)
                    .copied()
                    .ok_or_else(|| format!("joint {name} names unknown link {target}"))
            };
            joints.push(Joint {
                name: name.to_owned(),
                parent: link("parent")?,
                child: link("child")?,
                origin: origin_of(joint).map_err(|error| format!("joint {name}: {error}"))?,
            });
        }

        Ok(Self {
            name: root.attribute("name").unwrap_or("robot").to_owned(),
            links,
            joints,
        })
    }

    /// The single link no joint points at.
    pub fn root_link(&self) -> Result<usize, String> {
        let mut is_child = vec![false; self.links.len()];
        for joint in &self.joints {
            if std::mem::replace(&mut is_child[joint.child], true) {
                return Err(format!(
                    "link {} has two parent joints",
                    self.links[joint.child].name
                ));
            }
        }
        let mut roots = (0..self.links.len()).filter(|&link| !is_child[link]);
        match (roots.next(), roots.next()) {
            (Some(root), None) => Ok(root),
            (None, _) => Err("the joint graph has no root link".to_owned()),
            (Some(a), Some(b)) => Err(format!(
                "links {} and {} both have no parent joint",
                self.links[a].name, self.links[b].name
            )),
        }
    }

    /// Every link's pose in the root link's frame with all joints at zero.
    pub fn zero_pose(&self) -> Result<Vec<Pose>, String> {
        let root = self.root_link()?;
        let mut pose = vec![None; self.links.len()];
        pose[root] = Some(Pose::IDENTITY);
        let mut stack = vec![root];
        while let Some(link) = stack.pop() {
            let parent = pose[link].expect("pushed links are posed");
            for joint in self.joints.iter().filter(|joint| joint.parent == link) {
                if pose[joint.child].is_some() {
                    return Err(format!("joint {} closes a loop", joint.name));
                }
                pose[joint.child] = Some(parent.then(joint.origin));
                stack.push(joint.child);
            }
        }
        pose.into_iter()
            .enumerate()
            .map(|(link, pose)| {
                pose.ok_or_else(|| format!("link {} is not connected", self.links[link].name))
            })
            .collect()
    }
}

fn children<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    tag: &'static str,
) -> impl Iterator<Item = roxmltree::Node<'a, 'input>> {
    node.children()
        .filter(move |child| child.is_element() && child.tag_name().name() == tag)
}

fn colour_of(material: roxmltree::Node<'_, '_>) -> Result<Option<[f32; 4]>, String> {
    let Some(rgba) = children(material, "color")
        .next()
        .and_then(|colour| colour.attribute("rgba"))
    else {
        return Ok(None);
    };
    let values = numbers(rgba)?;
    let [r, g, b, a] = values[..] else {
        return Err(format!("rgba \"{rgba}\" is not four numbers"));
    };
    Ok(Some([r as f32, g as f32, b as f32, a as f32]))
}

fn origin_of(node: roxmltree::Node<'_, '_>) -> Result<Pose, String> {
    let Some(origin) = children(node, "origin").next() else {
        return Ok(Pose::IDENTITY);
    };
    let translation = origin.attribute("xyz").map_or(Ok(DVec3::ZERO), vec3)?;
    let [roll, pitch, yaw] = origin
        .attribute("rpy")
        .map_or(Ok(DVec3::ZERO), vec3)?
        .to_array();
    // URDF rpy is extrinsic roll about X, then pitch about Y, then yaw about Z.
    let rotation =
        DMat3::from_rotation_z(yaw) * DMat3::from_rotation_y(pitch) * DMat3::from_rotation_x(roll);
    Ok(Pose {
        rotation,
        translation,
    })
}

fn vec3(text: &str) -> Result<DVec3, String> {
    match numbers(text)?[..] {
        [x, y, z] => Ok(DVec3::new(x, y, z)),
        _ => Err(format!("\"{text}\" is not three numbers")),
    }
}

fn numbers(text: &str) -> Result<Vec<f64>, String> {
    text.split_whitespace()
        .map(|word| {
            word.parse::<f64>()
                .map_err(|_| format!("\"{word}\" is not a number"))
        })
        .collect()
}

/// Mesh paths are relative to the URDF, or `package://<package>/<path>`, which
/// resolves against the package directory the URDF sits in or under.
fn resolve_mesh(base: &Path, filename: &str) -> Result<PathBuf, String> {
    let Some(rest) = filename.strip_prefix("package://") else {
        return Ok(base.join(filename.strip_prefix("file://").unwrap_or(filename)));
    };
    let inside = rest.split_once('/').map_or("", |(_, path)| path);
    base.ancestors()
        .map(|dir| dir.join(inside))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            format!(
                "{filename} is not under any directory above {}",
                base.display()
            )
        })
}
