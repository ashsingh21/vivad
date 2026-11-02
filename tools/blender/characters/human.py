"""Everyday Delhi people built on an MPFB (MakeHuman, CC0) body: eight looks on one rig per sex.

blender -b -P tools/blender/characters/human.py -- <look> [--stage build|full]    looks: VARIANTS

Pipeline: MPFB body + game_engine rig + generic proxy -> MakeHuman community clothes fitted by MPFB
(so they inherit the body's bone weights; outfits.py, needs research/characters/scripts/fetch_mh.sh)
-> hidden body faces removed -> head subdivided, eyes split (head.py) -> procedural materials and
the garments' own textures -> bake one texture set per part
(head, body, outfit, eyes, mouth) + alpha hair cards -> four LODs ->
assets/models/characters/<look>_parts.glb, 4K head sources in build/textures. See finish.py.

The old one-mesh <variant>.glb is no longer written here: it stays as the engine's fallback.
BAKE_RES=512 caps the bakes for a quick test build.
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
from bl_ext.blender_org.mpfb.services.locationservice import LocationService  # noqa: E402

MPFB_DATA = LocationService.get_user_data()


def mpfb():
    from bl_ext.blender_org.mpfb.services.humanservice import HumanService
    return HumanService


# One rig per sex: these macros, base face and arm shape build it (skeleton.py), and every look on
# that sex wears it, so every clip fits all of them. Whatever makes a look its own person comes after
# the rig (build_body).
MAN = {
    "sex": "male",
    # MakeHuman macros (0..1). Lean, athletic warrior; mixed ancestry sliders give South Asian features.
    "macro": {"gender": 1.0, "age": 0.5, "muscle": 0.78, "weight": 0.52, "proportions": 0.75, "height": 0.58,
              "cupsize": 0.5, "firmness": 0.5, "race": {"asian": 0.22, "caucasian": 0.48, "african": 0.30}},
    "proxy": "proxymeshes/male_generic/male_generic.proxy",
    "skin": "skins/young_african_male/young_african_male.mhmat",
    "eyes": "brown",
    # Face detail targets (MPFB data/targets/<group>/<name>), weight 0..1: a narrower, longer
    # nose with a slight bridge, finer lips, eyes opened a little.
    "face": {"nose/nose-scale-horiz-decr": 0.7, "nose/nose-flaring-decr": 0.9, "nose/nose-point-width-decr": 0.7,
             "nose/nose-hump-incr": 0.5, "nose/nose-scale-vert-incr": 0.4,
             "mouth/mouth-lowerlip-volume-decr": 0.7, "mouth/mouth-upperlip-volume-decr": 0.6, "mouth/mouth-scale-horiz-decr": 0.3,
             "eyes/l-eye-height2-incr": 0.7, "eyes/r-eye-height2-incr": 0.7, "eyes/l-eye-scale-incr": 0.3, "eyes/r-eye-scale-incr": 0.3},
    # Arms and shoulders to the proportions of the mannequin the gun clips were captured on
    # (skeleton.py; MoCap Online's Motus Man: upper arm 30.3 cm, forearm 27.0, shoulder joints
    # 17.7 cm off the midline; this body had 27.1, 28.1 and 20.7), so a captured hold puts the
    # hands where the gun is.
    "shape": {"arms/measure-upperarm-length-incr": 0.46, "arms/measure-lowerarm-length-decr": 0.24,
              "torso/measure-shoulder-dist-decr": 1.0, "torso/torso-scale-horiz-decr": 0.5},
    "tone": (0.66, 0.44, 0.31),
    "lip": (0.42, 0.22, 0.20),
}
WOMAN = {
    "sex": "female",
    "macro": {"gender": 0.0, "age": 0.5, "muscle": 0.62, "weight": 0.48, "proportions": 0.75, "height": 0.52,
              "cupsize": 0.45, "firmness": 0.6, "race": {"asian": 0.22, "caucasian": 0.48, "african": 0.30}},
    "proxy": "proxymeshes/female_generic/female_generic.proxy",
    "skin": "skins/young_african_female/young_african_female.mhmat",
    "eyes": "brown",
    "face": {"nose/nose-scale-horiz-decr": 0.7, "nose/nose-flaring-decr": 0.9, "nose/nose-point-width-decr": 0.7,
             "mouth/mouth-lowerlip-volume-decr": 0.5, "mouth/mouth-upperlip-volume-decr": 0.5, "mouth/mouth-scale-horiz-decr": 0.2,
             "eyes/l-eye-height2-incr": 0.8, "eyes/r-eye-height2-incr": 0.8, "eyes/l-eye-scale-incr": 0.35, "eyes/r-eye-scale-incr": 0.35},
    # The mannequin's arm proportions scaled to her height (1.63 m to its 1.83): upper arm 23.6
    # to 26.9 cm, forearm 21.6 to 24.0 (her shoulders already match).
    "shape": {"arms/measure-upperarm-length-incr": 0.47, "arms/measure-lowerarm-length-incr": 0.37},
    "tone": (0.71, 0.48, 0.35),
    "lip": (0.48, 0.19, 0.20),
}


def both(*names, **weights):
    """A target on both sides of the face: both("cheek/{}-cheek-volume-incr", w=0.5) style names."""
    w = weights["w"]
    return {name.format(side): w for name in names for side in "lr"}


# The eight looks, in the game's `look % 8` order (characters.rs::LOOKS): odd is a woman. Keys of a look:
# flesh    macros set after rigging: the body on the bones, and the race mix and age of the face
# features face and body targets, also after rigging, on top of the sex's base face
# tone     skin sRGB; lip, hair_tint, brow_tint, stubble_tint and moustache_tint likewise
# beard    stubble, 0..1; moustache likewise, painted (face.moustache)
# top      the top's colour in the file, sRGB: the first of its row in characters.rs::TOPS
VARIANTS = {
    "male": {**MAN, "flesh": {"muscle": 0.42, "weight": 0.5},
             "hair": "hair/short04/short04.mhclo", "brows": "eyebrows/eyebrow006/eyebrow006.mhclo", "brow_strands": "mindfront_eyebrows_05",
             "beard": 1.0, "top": (0.52, 0.09, 0.14), "outfit": "male_street"},
    "female": {**WOMAN, "flesh": {"muscle": 0.38, "weight": 0.54},
               "hair": "hair/braid01/braid01.mhclo", "brows": "eyebrows/eyebrow010/eyebrow010.mhclo", "brow_strands": "mindfront_eyebrows_02",
               "top": (0.60, 0.06, 0.30), "outfit": "female_street"},
    "officeman": {**MAN, "flesh": {"muscle": 0.34, "weight": 0.62, "age": 0.58, "asian": 0.25, "caucasian": 0.5, "african": 0.25},
                  "features": {"head/head-round": 0.3, "head/head-fat-incr": 0.3, "chin/chin-width-incr": 0.3, **both("cheek/{}-cheek-volume-incr", w=0.35), **both("eyes/{}-eye-height2-decr", w=0.5),
                               "nose/nose-scale-horiz-incr": 0.5, "nose/nose-volume-incr": 0.3, "mouth/mouth-scale-horiz-incr": 0.3, "stomach/stomach-pregnant-incr": 0.2},
                  "tone": (0.62, 0.42, 0.30),
                  "hair": "hair/short01/short01.mhclo", "brows": "eyebrows/eyebrow006/eyebrow006.mhclo", "brow_strands": "mindfront_eyebrows_04",
                  "beard": 0.35, "moustache": 0.9, "top": (0.70, 0.79, 0.90), "outfit": "office_man"},
    "college": {**WOMAN, "flesh": {"muscle": 0.36, "weight": 0.42, "age": 0.44, "asian": 0.28, "caucasian": 0.47, "african": 0.25},
                "features": {"head/head-oval": 0.3, "chin/chin-width-decr": 0.3, **both("cheek/{}-cheek-bones-decr", w=0.2),
                             "nose/nose-scale-vert-decr": 0.3, "nose/nose-point-up": 0.3, "mouth/mouth-upperlip-volume-incr": 0.25},
                "tone": (0.76, 0.54, 0.40),
                "hair": "hair/ponytail01/ponytail01.mhclo", "brows": "eyebrows/eyebrow010/eyebrow010.mhclo", "brow_strands": "mindfront_eyebrows_03",
                "top": (0.85, 0.60, 0.12), "outfit": "college_girl"},
    "auto": {**MAN, "flesh": {"muscle": 0.3, "weight": 0.2, "age": 0.62, "asian": 0.15, "caucasian": 0.45, "african": 0.4},
             "features": {"head/head-rectangular": 0.3, **both("cheek/{}-cheek-volume-decr", w=0.6), **both("cheek/{}-cheek-bones-incr", w=0.5), **both("eyes/{}-eye-height2-decr", w=0.6),
                          "chin/chin-prominent-incr": 0.3, "nose/nose-scale-vert-incr": 0.3, "nose/nose-hump-incr": 0.4, "forehead/forehead-temple-decr": 0.4, "neck/measure-neck-circ-decr": 0.4},
             "tone": (0.48, 0.31, 0.22), "lip": (0.34, 0.19, 0.18),
             "hair": "hair/short02/short02.mhclo", "brows": "eyebrows/eyebrow006/eyebrow006.mhclo", "brow_strands": "mindfront_eyebrows_09",
             "beard": 1.0, "moustache": 0.8, "top": (0.46, 0.47, 0.47), "outfit": "auto_driver"},
    "aunty": {**WOMAN, "flesh": {"muscle": 0.3, "weight": 0.7, "age": 0.66, "cupsize": 0.4, "asian": 0.2, "caucasian": 0.45, "african": 0.35},
              "features": {"head/head-round": 0.5, "head/head-fat-incr": 0.4, "neck/neck-double-incr": 0.3, **both("cheek/{}-cheek-volume-incr", w=0.4), **both("eyes/{}-eye-height2-decr", w=0.4),
                           "nose/nose-scale-horiz-incr": 0.4, "nose/nose-volume-incr": 0.3},
              "tone": (0.64, 0.43, 0.31),
              "hair": "hair/braid01/braid01.mhclo", "brows": "eyebrows/eyebrow010/eyebrow010.mhclo", "brow_strands": "mindfront_eyebrows_02",
              "top": (0.06, 0.34, 0.26), "outfit": "aunty_suit"},
    "elder": {**MAN, "flesh": {"muscle": 0.26, "weight": 0.36, "age": 0.84, "asian": 0.2, "caucasian": 0.55, "african": 0.25},
              "features": {"head/head-oval": 0.3, "head/head-age-incr": 0.7, **both("cheek/{}-cheek-volume-decr", w=0.35), **both("eyes/{}-eye-height2-decr", w=0.5), **both("eyes/{}-eye-bag-incr", w=0.5),
                           **both("ears/{}-ear-scale-incr", w=0.4), "nose/nose-scale-vert-incr": 0.3, "nose/nose-volume-incr": 0.3, "forehead/forehead-temple-decr": 0.4, "neck/measure-neck-circ-decr": 0.3},
              "skin": "skins/old_african_male/old_african_male.mhmat",
              "tone": (0.58, 0.40, 0.30), "lip": (0.40, 0.25, 0.23), "hair_tint": (0.80, 0.79, 0.76), "scalp_tint": (0.50, 0.42, 0.36), "brow_tint": (0.5, 0.49, 0.47), "stubble_tint": (0.62, 0.60, 0.57), "moustache_tint": (0.74, 0.73, 0.70),
              "hair": "hair/short04/short04.mhclo", "brows": "eyebrows/eyebrow006/eyebrow006.mhclo", "brow_strands": "mindfront_eyebrows_01",
              "beard": 0.5, "moustache": 0.9, "top": (0.90, 0.89, 0.85), "outfit": "elder_kurta"},
    "officewoman": {**WOMAN, "flesh": {"muscle": 0.4, "weight": 0.46, "age": 0.56, "cupsize": 0.3, "asian": 0.2, "caucasian": 0.55, "african": 0.25},
                    "features": {"head/head-diamond": 0.3, "nose/nose-scale-vert-incr": 0.25, "chin/chin-prominent-incr": 0.2, **both("cheek/{}-cheek-bones-incr", w=0.3)},
                    "tone": (0.68, 0.45, 0.33),
                    "hair": "pack:hair01/hair/toigo_blunt_bob", "brows": "eyebrows/eyebrow010/eyebrow010.mhclo", "brow_strands": "mindfront_eyebrows_03",
                    "top": (0.88, 0.88, 0.85), "outfit": "office_woman"},
}


def clear_scene():
    bpy.ops.wm.read_homefile(use_empty=True)


TARGETS = LocationService.get_mpfb_data("targets")


def build_body(cfg):
    hs = mpfb()
    body = hs.create_human(macro_detail_dict=cfg["macro"])
    from bl_ext.blender_org.mpfb.entities.objectproperties import HumanObjectProperties
    from bl_ext.blender_org.mpfb.services.targetservice import TargetService
    for name, w in {**cfg.get("face", {}), **cfg.get("shape", {})}.items():
        TargetService.load_target(body, os.path.join(TARGETS, f"{name}.target.gz"), weight=w)
    rig = hs.add_builtin_rig(body, "game_engine")
    for name, value in cfg.get("flesh", {}).items():
        HumanObjectProperties.set_value(name, value, entity_reference=body)
    TargetService.reapply_macro_details(body)
    for name, w in cfg.get("features", {}).items():
        TargetService.load_target(body, os.path.join(TARGETS, f"{name}.target.gz"), weight=w)
    proxy = hs.add_mhclo_asset(os.path.join(MPFB_DATA, cfg["proxy"]), body, asset_type="Proxymeshes", subdiv_levels=0)
    hs.set_character_skin(os.path.join(MPFB_DATA, cfg["skin"]), body, skin_type="GAMEENGINE")
    parts = {"proxy": proxy}
    for kind, rel in (("Eyes", "eyes/high-poly/high-poly.mhclo"), ("Eyebrows", cfg["brows"]),
                      ("Eyelashes", "eyelashes/eyelashes01/eyelashes01.mhclo"), ("Hair", cfg["hair"]),
                      ("Teeth", "teeth/teeth_base/teeth_base.mhclo"), ("Tongue", "tongue/tongue01/tongue01.mhclo")):
        path = outfits.pack_path(rel) if rel.startswith("pack:") else os.path.join(MPFB_DATA, rel)
        parts[kind.lower()] = hs.add_mhclo_asset(path, body, asset_type=kind, subdiv_levels=0, material_type="MAKESKIN")
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


def delete_hidden_body(proxy, covers, gap=0.05):
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
    ctx.body, ctx.parts = body, parts
    ctx.brow_top = max((parts["eyebrows"].matrix_world @ v.co).z for v in parts["eyebrows"].data.vertices)
    items = getattr(outfits, cfg["outfit"])(ctx)          # list of (object, role) where role in {"cover", "accessory", "hair_replace"}
    items += face.decorate(cfg, proxy, parts, body, ctx)
    proxy["brows"] = face.brow_mask(proxy, body, os.path.join(outfits.PACKS, "eyebrows01", "x", "eyebrows", cfg["brow_strands"], f"{cfg['brow_strands']}.mhclo")).name
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
    finish.finish_character(variant, rig, proxy, parts, items, ROOT, cfg)


if __name__ == "__main__":
    main()
