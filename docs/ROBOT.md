# Robot: a URDF model standing in the match

`crates/urdf_model` reads a robot description (URDF links, joints, STL meshes)
and stands it in front of a spawn as a script model. Off unless configured; a
match without it loads exactly as before.

## Get a robot and turn it on

The Unitree G1 description is BSD-3 and comes from Unitree's repository;
`context/` is ignored by git, so the meshes never enter the tree:

```bash
git clone --depth 1 --filter=blob:none --sparse \
  https://github.com/unitreerobotics/unitree_ros.git context/externals/unitree_ros
git -C context/externals/unitree_ros sparse-checkout set robots/g1_description
```

In `.env` (absolute path):

```bash
IW4L_ROBOT_URDF=/…/context/externals/unitree_ros/robots/g1_description/g1_29dof_rev_1_0.urdf
# IW4L_ROBOT_ORIGIN="-334 1580 -76"   optional, map units, feet height
# IW4L_ROBOT_YAW=0                    optional, degrees, with ORIGIN
```

`make map mp_boneyard`; the log line `urdf robot …` says where it stands. Every
machine in a match needs the same setting: the robot takes an entity slot.
`cargo run --release -p urdf_model --example inspect -- <urdf>` prints what the
import makes of a description without starting the game.

## What happens

* **Geometry** (`mesh.rs`): each link's STL soup is welded, simplified with
  meshoptimizer to 1.5 mm deviation (G1: 393k → 19.5k triangles), and given
  normals that break at edges sharper than 40°.
* **Skeleton** (`skel.rs`): `tag_origin` plus one rigid bone per link, all
  joints at zero, feet on z = 0, metres → inches, STL winding flipped to IW4's
  clockwise front faces. Per-link boxes are the bone collision.
* **Materials** (`install.rs`): one stand-in per URDF colour, cloned from the
  common_mp `mc/mtl_weapon_claymore` technique with a solid colour map, flat
  normal map and fixed specular/env constants.
* **Hook**: `assets::session_load::match_walk`, before material ids are
  provisioned — the XModel enters `map_xmodel_scene_assets`, the placement
  `script_model_instances`. Lighting is sampled from the map's light grid.

## Not yet

Players walk through it (no movement brush). It does not move: the bones are
there, nothing animates them. Bullet hits on the link boxes are not verified.
