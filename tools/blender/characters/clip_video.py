"""A video of clips as they play, for looking at motion rather than stills: the animation build's
rig and body (build/anims_<variant>.blend, saved by anims.py), the bandook on the weapon bone as the
game hangs it, each clip looped for about four seconds, seen from behind (where the game's camera
sits) and from the side, its name in the corner.

blender -b -P tools/blender/characters/clip_video.py -- male out.mp4 [clip,clip,...]

A clip named in SEQUENCES is a move played through as in the game instead: its clips one after
another, cross-faded at the game's rate, the body travelling over a gridded floor at the game's
speeds (the slide: a sprint, the slide slowing from SLIDE_SPEED, a crouch walk out of it).
"""

import math
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, HERE)

import bpy  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import skeleton  # noqa: E402

CLIPS = ["rifle_slide_move", "rifle_idle", "rifle_walk_f", "rifle_run_f", "rifle_sprint_f", "rifle_run_l", "rifle_run_r", "rifle_run_b",
         "rifleaim_idle", "rifleaim_walk_f", "rifleaim_run_f", "rifle_crouch_idle", "rifle_crouch_f", "rifleaim_crouch_f",
         "rifle_slide"]
FPS = 30
# crates/game/src/animation.rs FADE_RATE; crates/shared/src/sim/movement.rs.
FADE_RATE = 12.0
SLIDE_IN = SLIDE_OUT = 8  # frames: anims.py
SPRINT, SLIDE_SPEED, SLIDE_FRICTION, SLIDE_END, CROUCH = 8.6, 10.5, 6.5, 3.6, 1.4
# (sprint, slide, crouch walk) clips.
SEQUENCES = {"slide_move": ("sprint_f", "slide", "crouch_f"),
             "rifle_slide_move": ("rifle_sprint_f", "rifle_slide", "rifle_crouch_f")}


def main():
    args = sys.argv[sys.argv.index("--") + 1:]
    variant, out = args[0], os.path.abspath(args[1])
    clips = args[2].split(",") if len(args) > 2 else CLIPS
    # The gun, its geometry as the game has it (glTF, read back into Blender's axes), imported in a
    # clean scene (the importer trips on the saved file's) and carried over.
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
    # On the weapon bone: the bone's frame turned back from glTF's (skeleton.TO_GLTF).
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
    sc.render.engine = "BLENDER_WORKBENCH"
    sc.display.shading.light = "STUDIO"
    sc.display.shading.color_type = "TEXTURE"
    sc.display.shading.show_shadows = True
    sc.render.resolution_x, sc.render.resolution_y = 640, 640
    sc.render.fps = FPS
    world = bpy.data.worlds.new("w")
    world.color = (0.32, 0.34, 0.38)
    sc.world = world
    # A floor to see the feet against.
    bpy.ops.mesh.primitive_plane_add(size=200)
    floor = bpy.context.active_object
    floor.color = (0.55, 0.52, 0.48, 1)
    # A one-metre grid on it, so travel shows.
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
    cam.data.lens = 40
    tmp = tempfile.mkdtemp()
    labels, n = [], 0
    def shoot(name):
        nonlocal n
        hip = arm.matrix_world @ arm.pose.bones["pelvis"].head
        t = Vector((hip.x, hip.y, 0.95))
        # Behind and to the right, a little above (where the game's camera sits), and the side.
        for view, off in (("a", Vector((1.6, 2.6, 0.55))), ("b", Vector((3.2, 0.0, 0.2)))):
            cam.location = t + off
            cam.rotation_euler = (t - cam.location).to_track_quat("-Z", "Y").to_euler()
            sc.render.filepath = os.path.join(tmp, f"{n:05d}{view}.png")
            bpy.ops.render.render(write_still=True)
        labels.append(name)
        n += 1

    for name in clips:
        if name in SEQUENCES:
            for _ in sequence(arm, SEQUENCES[name]):
                shoot(name)
    with open(os.path.join(tmp, "labels.txt"), "w") as fh:
        fh.write("\n".join(labels))
    print(f"[video] {n} frames in {tmp}")
    label(tmp, labels)
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-framerate", str(FPS), "-i", os.path.join(tmp, "%05d.png"),
                    "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "23", out], check=True)
    print(f"[video] {out}: {len(clips)} clips, {n / FPS:.0f} s")


def pose_of(arm, act, f):
    """Every bone's (rotation, location) in `act` at frame `f` (wrapped into its loop)."""
    arm.animation_data.action = act
    f0, f1 = map(int, act.frame_range)
    bpy.context.scene.frame_set(f0 + int(f) % max(f1 - f0, 1))
    return {pb.name: (pb.rotation_quaternion.copy(), pb.location.copy()) for pb in arm.pose.bones}


def sequence(arm, names):
    """Play `names` (sprint, slide, crouch walk) through as the game does, yielding once per frame
    with the rig posed and moved: one second's sprint, the slide from SLIDE_SPEED slowing by
    SLIDE_FRICTION to SLIDE_END (its clip dropping in, then its hold looped), its get-up, a second's
    crouch walk; each clip faded in at FADE_RATE, the gaits' cycles advanced by distance travelled
    at the clip's own speed."""
    acts = [bpy.data.actions[n] for n in names]
    speeds = [SPRINT, 0.0, CROUCH]
    weights = [1.0, 0.0, 0.0]
    phase = [0.0, 0.0, 0.0]
    dt, y, v, stage, t = 1 / FPS, 0.0, SPRINT, 0, 0.0
    k = min(dt * FADE_RATE, 1.0)
    f0, f1 = map(int, acts[1].frame_range)
    hold = f1 - f0 - SLIDE_IN - SLIDE_OUT
    while True:
        if stage == 0 and t >= 1.0:
            stage, v, t = 1, SLIDE_SPEED, 0.0
        elif stage == 1:
            v -= SLIDE_FRICTION * dt
            if v < SLIDE_END:
                stage, v, t = 2, CROUCH, 0.0
        elif stage == 2 and t * FPS >= SLIDE_OUT:
            stage, t = 3, 0.0
        elif stage == 3 and t >= 1.0:
            return
        weights = [w + ((1.0 if i == (0, 1, 1, 2)[stage] else 0.0) - w) * k for i, w in enumerate(weights)]
        y -= v * dt
        for i in (0, 2):
            phase[i] += dt * FPS * v / speeds[i]
        x = t * FPS
        phase[1] = (x if x < SLIDE_IN else SLIDE_IN + (x - SLIDE_IN) % hold) if stage == 1 else \
            (f1 - f0 - SLIDE_OUT + min(x, SLIDE_OUT - 0.01)) if stage == 2 else phase[1]
        poses = [pose_of(arm, a, p) for a, p in zip(acts, phase)]
        arm.animation_data.action = None
        total = 0.0
        for w, pose in zip(weights, poses):
            if w < 1e-3:
                continue
            total += w
            for pb in arm.pose.bones:
                q, l = pose[pb.name]
                if total == w:
                    pb.rotation_quaternion, pb.location = q, l
                else:
                    pb.rotation_quaternion = pb.rotation_quaternion.slerp(q, w / total)
                    pb.location = pb.location.lerp(l, w / total)
        arm.location = (0, y, 0)
        bpy.context.view_layer.update()
        yield
        t += dt


LABEL = """
import sys
from PIL import Image, ImageDraw
tmp = sys.argv[1]
names = open(f"{tmp}/labels.txt").read().split("\\n")
for i, n in enumerate(names):
    a, b = Image.open(f"{tmp}/{i:05d}a.png").convert("RGB"), Image.open(f"{tmp}/{i:05d}b.png").convert("RGB")
    im = Image.new("RGB", (a.width + b.width, a.height))
    im.paste(a, (0, 0))
    im.paste(b, (a.width, 0))
    p = f"{tmp}/{i:05d}.png"
    d = ImageDraw.Draw(im)
    d.rectangle((0, 0, 12 + 9 * len(n), 26), fill=(0, 0, 0))
    d.text((6, 7), n, fill=(255, 230, 80))
    im.save(p)
"""


def label(tmp, labels):
    """Each frame's clip name in its corner (Pillow, in the system Python: Blender's has none)."""
    subprocess.run(["python3", "-c", LABEL, tmp], check=True)


if __name__ == "__main__":
    main()
