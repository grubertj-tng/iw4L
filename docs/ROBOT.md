# Robot: a URDF model in the match

`crates/urdf_model` reads a robot description (URDF links, joints, STL meshes)
and puts it in a match: standing in front of a spawn as a script model, and/or
as the soldier body of one or both teams. Off unless configured; a match
without it loads exactly as before.

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
# IW4L_ROBOT_BODY=all                 all | allies | axis: those soldiers are robots
# IW4L_ROBOT_PROP=off                 no robot standing at the spawn
# IW4L_ROBOT_ORIGIN="-334 1580 -76"   map units, feet height; IW4L_ROBOT_YAW degrees
```

The log lines `urdf robot …` say what was installed and where. Every machine in
a match needs the same settings. `cargo run --release -p urdf_model --example
inspect -- <urdf>` prints what the import makes of a description.

## What happens

* **Geometry** (`mesh.rs`): each link's STL is welded, simplified with
  meshoptimizer to 1.5 mm (G1: 393k → 19.5k triangles), normals break at 40°.
* **Prop** (`skel.rs`): `tag_origin` plus one rigid bone per link, joints at
  zero, feet on z = 0, inches, winding flipped to IW4's clockwise fronts.
* **Body** (`body.rs`): the kit's soldier skeleton, hit boxes and tags are kept;
  a rig table (`UNITREE_G1`) puts each link on a soldier bone. Segments are
  scaled to soldier size and stretched along their length so the robot's
  joints land on the soldier's; the head turns with `j_head`, no separate head
  model. Player animations, aiming and hit locations work unchanged.
* **Materials** (`install.rs`): one stand-in per URDF colour on the common_mp
  `mc/mtl_weapon_claymore` technique: solid colour, flat normal, fixed specular.
* **Hook**: `assets::session_load::match_walk` before material ids are
  provisioned — XModel, placement, body and kit choice join the match there.

## Not yet

Players walk through the standing robot and nothing animates it. Bodies have one
level of detail. First-person arms stay the soldier's. Another robot needs its
own rig table.
