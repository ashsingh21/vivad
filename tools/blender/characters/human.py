"""Generic warrior characters (male / female) built on an MPFB (MakeHuman, CC0) body.

blender -b -P tools/blender/characters/human.py -- male|female [--stage build|full]

Pipeline: MPFB body + game_engine rig + generic proxy -> garments grown from the body's own
topology (so they inherit its bone weights) -> jewellery and turban -> hidden body faces removed
-> procedural materials -> bake to one opaque texture set + alpha hair cards -> LODs -> GLB.
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bmesh  # noqa: E402
import bpy  # noqa: E402
import numpy as np  # noqa: E402
from mathutils import Vector  # noqa: E402
from mathutils.bvhtree import BVHTree  # noqa: E402

import face  # noqa: E402
import garments  # noqa: E402
import outfits  # noqa: E402

MPFB_DATA = os.path.expanduser("~/.config/blender/5.2/mpfb/data")


def mpfb():
    from bl_ext.blender_org.mpfb.services.humanservice import HumanService
    return HumanService


VARIANTS = {
    "male": {
        # MakeHuman macros (0..1). Lean, athletic warrior; mixed ancestry sliders give South Asian features.
        "macro": {"gender": 1.0, "age": 0.5, "muscle": 0.78, "weight": 0.52, "proportions": 0.75, "height": 0.58,
                  "cupsize": 0.5, "firmness": 0.5, "race": {"asian": 0.22, "caucasian": 0.48, "african": 0.30}},
        "proxy": "proxymeshes/male_generic/male_generic.proxy",
        "skin": "skins/young_african_male/young_african_male.mhmat",
        "hair": "hair/short02/short02.mhclo",
        "brows": "eyebrows/eyebrow006/eyebrow006.mhclo",
        "eyes": "brown",
        # Face detail targets (MPFB data/targets/<group>/<name>), weight 0..1: a narrower, longer
        # nose with a slight bridge, finer lips, eyes opened a little.
        "face": {"nose/nose-scale-horiz-decr": 0.7, "nose/nose-flaring-decr": 0.9, "nose/nose-point-width-decr": 0.7,
                 "nose/nose-hump-incr": 0.5, "nose/nose-scale-vert-incr": 0.4,
                 "mouth/mouth-lowerlip-volume-decr": 0.7, "mouth/mouth-upperlip-volume-decr": 0.6, "mouth/mouth-scale-horiz-decr": 0.3,
                 "eyes/l-eye-height2-incr": 0.7, "eyes/r-eye-height2-incr": 0.7, "eyes/l-eye-scale-incr": 0.3, "eyes/r-eye-scale-incr": 0.3},
        "outfit": outfits.male_street,
    },
    "female": {
        "macro": {"gender": 0.0, "age": 0.5, "muscle": 0.62, "weight": 0.48, "proportions": 0.75, "height": 0.52,
                  "cupsize": 0.45, "firmness": 0.6, "race": {"asian": 0.22, "caucasian": 0.48, "african": 0.30}},
        "proxy": "proxymeshes/female_generic/female_generic.proxy",
        "skin": "skins/young_african_female/young_african_female.mhmat",
        "hair": "hair/long01/long01.mhclo",
        "brows": "eyebrows/eyebrow010/eyebrow010.mhclo",
        "eyes": "brown",
        "face": {"nose/nose-scale-horiz-decr": 0.7, "nose/nose-flaring-decr": 0.9, "nose/nose-point-width-decr": 0.7,
                 "mouth/mouth-lowerlip-volume-decr": 0.5, "mouth/mouth-upperlip-volume-decr": 0.5, "mouth/mouth-scale-horiz-decr": 0.2,
                 "eyes/l-eye-height2-incr": 0.8, "eyes/r-eye-height2-incr": 0.8, "eyes/l-eye-scale-incr": 0.35, "eyes/r-eye-scale-incr": 0.35},
        "outfit": outfits.female_street,
    },
}


def clear_scene():
    bpy.ops.wm.read_homefile(use_empty=True)


TARGETS = os.path.expanduser("~/.config/blender/5.2/extensions/blender_org/mpfb/data/targets")


def build_body(cfg):
    hs = mpfb()
    body = hs.create_human(macro_detail_dict=cfg["macro"])
    from bl_ext.blender_org.mpfb.services.targetservice import TargetService
    for name, w in cfg.get("face", {}).items():
        TargetService.load_target(body, os.path.join(TARGETS, f"{name}.target.gz"), weight=w)
    rig = hs.add_builtin_rig(body, "game_engine")
    proxy = hs.add_mhclo_asset(os.path.join(MPFB_DATA, cfg["proxy"]), body, asset_type="Proxymeshes", subdiv_levels=0)
    hs.set_character_skin(os.path.join(MPFB_DATA, cfg["skin"]), body, skin_type="GAMEENGINE")
    parts = {"proxy": proxy}
    for kind, rel in (("Eyes", "eyes/low-poly/low-poly.mhclo"), ("Eyebrows", cfg["brows"]),
                      ("Eyelashes", "eyelashes/eyelashes01/eyelashes01.mhclo"), ("Hair", cfg["hair"])):
        parts[kind.lower()] = hs.add_mhclo_asset(os.path.join(MPFB_DATA, rel), body, asset_type=kind, subdiv_levels=0,
                                                 material_type="MAKESKIN")
    # Remove MPFB's subdivision/mask helpers from the parts; keep only the armature deform.
    for o in parts.values():
        for m in list(o.modifiers):
            if m.type != "ARMATURE":
                o.modifiers.remove(m)
    return body, rig, parts


def bone_weight_map(obj):
    """{vertex index: {group name: weight}} for the object's deform groups."""
    names = {g.index: g.name for g in obj.vertex_groups}
    out = {}
    for v in obj.data.vertices:
        out[v.index] = {names[g.group]: g.weight for g in v.groups if g.weight > 0.0}
    return out


def delete_hidden_body(proxy, covers, gap=0.09):
    """Delete body faces whose every vertex is covered by a garment within `gap` along its normal.
    Saves triangles and prevents skin poking through cloth when animated."""
    trees = []
    for c in covers:
        bm = bmesh.new()
        bm.from_mesh(c.data)
        trees.append(BVHTree.FromBMesh(bm))
        bm.free()
    me = proxy.data
    covered = np.zeros(len(me.vertices), dtype=bool)
    for v in me.vertices:
        for t in trees:
            hit = t.ray_cast(v.co + v.normal * 0.002, v.normal, gap)
            if hit[0] is not None:
                covered[v.index] = True
                break
    bm = bmesh.new()
    bm.from_mesh(me)
    bm.verts.ensure_lookup_table()
    doomed = [f for f in bm.faces if all(covered[v.index] for v in f.verts)]
    bmesh.ops.delete(bm, geom=doomed, context="FACES")
    bm.to_mesh(me)
    bm.free()
    return len(doomed)


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else ["male"]
    variant = argv[0]
    stage = argv[argv.index("--stage") + 1] if "--stage" in argv else "full"
    cfg = VARIANTS[variant]
    clear_scene()
    body, rig, parts = build_body(cfg)
    proxy = parts["proxy"]
    ctx = garments.BodyContext(proxy, rig, bone_weight_map(proxy))
    ctx.brow_top = max((parts["eyebrows"].matrix_world @ v.co).z for v in parts["eyebrows"].data.vertices)
    items = cfg["outfit"](ctx)          # list of (object, role) where role in {"cover", "accessory", "hair_replace"}
    items += face.decorate(variant, proxy, parts, ctx)
    if any(role == "hair_replace" for _, role in items):
        # Short hair shows below the turban's lower edge (no bare scalp); the rest is inside it.
        cut = face.trim_hair(parts["hair"], ctx.brow_top + 0.02)
        print(f"[{variant}] hair: {cut} faces hidden by the turban removed")
    covers = [o for o, role in items if role == "cover"]
    removed = delete_hidden_body(proxy, covers)
    print(f"[{variant}] removed {removed} hidden body faces")
    # The MakeHuman basemesh is only needed for fitting.
    bpy.data.objects.remove(body)
    out_blend = os.path.join(ROOT, "build", f"char_{variant}_{stage}.blend")
    if stage == "build":
        garments.preview_materials(proxy, parts, items)
        bpy.ops.wm.save_as_mainfile(filepath=out_blend)
        print(f"[{variant}] saved {out_blend}")
        return
    import finish
    finish.finish_character(variant, rig, proxy, parts, items, ROOT)


if __name__ == "__main__":
    main()
