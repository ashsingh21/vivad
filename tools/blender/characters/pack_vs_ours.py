"""MoCap Online's Rifle Basic clips side by side: on the left the pack's own mannequin (MotusMan
v55) playing each file exactly as bought, its M4 on its weapon bone; on the right our body playing
the game clip the build makes of it (build/anims_<variant>.blend, anims.py's MOTUS_CLIPS). Each at
its own pace, looped for four seconds, seen from behind-right (the game's camera) and the front.
What the pack really looks like, and how far the build has taken it from that.

  blender -b -P tools/blender/characters/pack_vs_ours.py -- male out.mp4 [clip,clip,...]
"""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, HERE)

import bpy  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import anims  # noqa: E402
import skeleton  # noqa: E402

FPS = 30
SECONDS = 4
CLIPS = ["rifle_idle", "rifle_aim", "rifle_walk_f", "rifle_run_f", "rifle_run_l", "rifle_run_b", "rifle_crouch_f", "rifleaim_walk_f", "rifleaim_run_f"]
# The pack's mannequin this far to our body's left.
APART = 1.4
LABEL = r"""
import sys, os
from PIL import Image, ImageDraw
tmp = sys.argv[1]
labels = open(os.path.join(tmp, "labels.txt")).read().split("\n")
for i, name in enumerate(labels):
    a, b = Image.open(os.path.join(tmp, f"{i:05d}a.png")), Image.open(os.path.join(tmp, f"{i:05d}b.png"))
    out = Image.new("RGB", (a.width + b.width, a.height))
    out.paste(a, (0, 0)); out.paste(b, (a.width, 0))
    d = ImageDraw.Draw(out)
    d.rectangle((0, 0, out.width, 26), fill=(0, 0, 0))
    d.text((8, 7), f"{name}    MotusMan (black and red): the pack as bought    in the vest: ours, as the game plays it", fill=(255, 255, 255))
    out.save(os.path.join(tmp, f"{i:05d}.png"))
"""


def main():
    args = sys.argv[sys.argv.index("--") + 1:]
    variant, out = args[0], os.path.abspath(args[1])
    clips = args[2].split(",") if len(args) > 2 else CLIPS
    files = {name: (file, opts) for name, file, _speed, opts in anims.MOTUS_CLIPS}
    # Our gun, as clip_video.py hangs it (glTF read in a clean scene, carried over).
    gun_blend = os.path.join(tempfile.mkdtemp(), "gun.blend")
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=os.path.join(ROOT, "assets", "models", "weapons", "bandook.glb"))
    bpy.ops.wm.save_as_mainfile(filepath=gun_blend)
    bpy.ops.wm.open_mainfile(filepath=os.path.join(ROOT, "build", f"anims_{variant}.blend"))
    arm = bpy.data.objects["Human.rig"]
    sc = bpy.context.scene
    objs = set(bpy.data.objects)
    with bpy.data.libraries.load(gun_blend) as (src, dst):
        dst.objects = [n for n in src.objects if n.endswith("LOD0")]
    for o in dst.objects:
        sc.collection.objects.link(o)
    for o in [o for o in bpy.data.objects if o not in objs]:
        if o.type == "MESH" and o.name.endswith("LOD0"):
            o.parent = None
            o.matrix_world = Matrix.Identity(4)
            c = o.constraints.new("CHILD_OF")
            c.target, c.subtarget = arm, skeleton.WEAPON
            c.set_inverse_pending = False
            c.inverse_matrix = Matrix.Identity(4)
            o.matrix_basis = skeleton.TO_GLTF.copy()
        else:
            bpy.data.objects.remove(o)
    # The pack's mannequin, and its M4 on hand_r_wep (skeleton.GUN_ON_BONE: the pack's own hold).
    if bpy.context.object and bpy.context.object.mode != "OBJECT":
        bpy.ops.object.mode_set(mode="OBJECT")
    before = set(bpy.data.objects)
    bpy.ops.import_scene.fbx(filepath=os.path.join(skeleton.PACK, "MotusMan", "MotusMan_v55.fbx"))
    motus = next(o for o in bpy.data.objects if o not in before and o.type == "ARMATURE")
    motus.animation_data_clear()
    motus.location.x -= APART
    before = set(bpy.data.objects)
    bpy.ops.import_scene.fbx(filepath=os.path.join(skeleton.PACK, "M4", "M4_Rifle_01.fbx"))
    m4 = []
    for o in [o for o in bpy.data.objects if o not in before]:
        if o.type != "MESH":
            bpy.data.objects.remove(o)
            continue
        o.parent = None
        # Placed each frame (hold_m4) from the transform it came in with (FBX's quarter turn, its
        # centimetres).
        m4.append((o, o.matrix_world.copy()))
    sc.render.engine = "BLENDER_WORKBENCH"
    sc.display.shading.light = "STUDIO"
    sc.display.shading.color_type = "TEXTURE"
    sc.display.shading.show_shadows = True
    sc.render.resolution_x, sc.render.resolution_y = 640, 560
    sc.render.fps = FPS
    world = bpy.data.worlds.new("w")
    world.color = (0.32, 0.34, 0.38)
    sc.world = world
    bpy.ops.mesh.primitive_plane_add(size=200)
    floor = bpy.context.active_object
    img = bpy.data.images.new("grid", 1024, 1024)
    img.generated_type = "UV_GRID"
    mat = bpy.data.materials.new("floor")
    mat.use_nodes = True
    tex = mat.node_tree.nodes.new("ShaderNodeTexImage")
    tex.image = img
    mat.node_tree.links.new(tex.outputs["Color"], mat.node_tree.nodes["Principled BSDF"].inputs["Base Color"])
    floor.data.materials.append(mat)
    for loop in floor.data.uv_layers.active.data:
        loop.uv *= 25
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    sc.collection.objects.link(cam)
    sc.camera = cam
    cam.data.lens = 32
    tmp = tempfile.mkdtemp()
    labels, n = [], 0
    for name in clips:
        if name not in files or name not in bpy.data.actions:
            print(f"[pack] no {name} (in the pack's list: {name in files}; built: {name in bpy.data.actions})")
            continue
        file, opts = files[name]
        before, acts = set(bpy.data.objects), set(bpy.data.actions)
        bpy.ops.import_scene.fbx(filepath=os.path.join(anims.MOTUS, f"{file}.fbx"))
        new_acts = [a for a in bpy.data.actions if a not in acts]
        for o in [o for o in bpy.data.objects if o not in before]:
            bpy.data.objects.remove(o)
        sc.render.fps, sc.render.fps_base = FPS, 1.0
        pack = new_acts[0]
        # Posed by hand at its own frame (motus_frame); as its action, a render would put it back
        # at the scene's.
        motus.animation_data_clear()
        ours = bpy.data.actions[name]
        arm.animation_data.action = ours
        arm.location = (0, 0, 0)
        p0, p1 = map(int, pack.frame_range)
        # The pack's window where the build takes one (an idle settling in from another pose).
        if opts.get("window"):
            p0, p1 = opts["window"]
        o0, o1 = map(int, ours.frame_range)
        for i in range(SECONDS * FPS):
            # Each looping at its own length.
            fp = p0 + i % max(p1 - p0, 1)
            fo = o0 + i % max(o1 - o0, 1)
            sc.frame_set(fo)
            motus_frame(motus, pack, fp)
            hold_m4(motus, m4)
            mid = Vector((-APART / 2, 0, 0.95))
            # Behind and to the right, a little above (the game's camera), and three-quarters from the front (from
            # the side the two would stand in a line).
            for view, off in (("a", Vector((1.9, 3.6, 0.7))), ("b", Vector((3.4, -3.8, 0.35)))):
                cam.location = mid + off
                cam.rotation_euler = (mid - cam.location).to_track_quat("-Z", "Y").to_euler()
                sc.render.filepath = os.path.join(tmp, f"{n:05d}{view}.png")
                bpy.ops.render.render(write_still=True)
            labels.append(name)
            n += 1
        print(f"[pack] {name}: {file} frames {p0}-{p1} against ours {o0}-{o1}")
    with open(os.path.join(tmp, "labels.txt"), "w") as fh:
        fh.write("\n".join(labels))
    subprocess.run(["python3", "-c", LABEL, tmp], check=True)
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-framerate", str(FPS), "-i", os.path.join(tmp, "%05d.png"),
                    "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "23", out], check=True)
    print(f"[pack] {out}: {n / FPS:.0f} s")


def hold_m4(rig, m4):
    """The rifle on the weapon bone as skeleton.py fits it: the bone's frame (no scale), then
    GUN_ON_BONE, then the rifle as imported."""
    loc, rot, _ = (rig.matrix_world @ rig.pose.bones["hand_r_wep"].matrix).decompose()
    for o, imported in m4:
        o.matrix_world = Matrix.LocRotScale(loc, rot, None) @ skeleton.GUN_ON_BONE @ imported


def motus_frame(rig, act, f):
    """Pose `rig` from `act` at frame `f`, whatever the scene's frame (ours plays at another)."""
    for pb in rig.pose.bones:
        pb.rotation_mode = "QUATERNION"
    for fc in anims.fcurves_of(act):
        path = fc.data_path
        # The weapon bones come with empty curves (no keys), which read 0: a gun scaled to nothing.
        if not path.startswith('pose.bones["') or not fc.keyframe_points:
            continue
        bone = path.split('"')[1]
        pb = rig.pose.bones.get(bone)
        if pb is None:
            continue
        prop = path.rsplit(".", 1)[1]
        v = fc.evaluate(f)
        if prop == "location":
            pb.location[fc.array_index] = v
        elif prop == "rotation_quaternion":
            pb.rotation_quaternion[fc.array_index] = v
        elif prop == "rotation_euler":
            pb.rotation_mode = "XYZ"
            pb.rotation_euler[fc.array_index] = v
        elif prop == "scale":
            pb.scale[fc.array_index] = v
    bpy.context.view_layer.update()


if __name__ == "__main__":
    main()
