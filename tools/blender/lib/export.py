"""Final game material, LODs, UV packing and GLB export."""

import hashlib
import json
import os
import shutil
import tempfile

import bpy

from geo import tri_count


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.unit_settings.system = "METRIC"
    scene.unit_settings.scale_length = 1.0


def pack_uvs(obj, margin=0.006):
    """Keep the metric unwrap as 'detail_uv' (shader pattern space) and pack 'UVMap' into 0-1.

    Islands are never rotated so the blade grain keeps a consistent tangent direction.
    """
    me = obj.data
    src = me.uv_layers["UVMap"]
    detail = me.uv_layers.new(name="detail_uv")
    for a, b in zip(src.data, detail.data):
        b.uv = a.uv
    me.uv_layers.active = src
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.select_all(action="SELECT")
    bpy.ops.uv.pack_islands(udim_source="CLOSEST_UDIM", rotate=False, scale=True, margin_method="FRACTION",
                            margin=margin, shape_method="CONCAVE")
    bpy.ops.object.mode_set(mode="OBJECT")


def gltf_output_group():
    """Node group the glTF exporter recognises for the occlusion map."""
    name = "glTF Material Output"
    grp = bpy.data.node_groups.get(name)
    if grp is None:
        grp = bpy.data.node_groups.new(name, "ShaderNodeTree")
        grp.interface.new_socket("Occlusion", in_out="INPUT", socket_type="NodeSocketFloat")
    return grp


def game_material(name, images):
    """One material, one texture set: basecolor (sRGB), ORM (R=AO, G=rough, B=metal), tangent normal."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    bsdf = nt.nodes.new("ShaderNodeBsdfPrincipled")
    nt.links.new(bsdf.outputs["BSDF"], out.inputs["Surface"])

    def tex(img):
        n = nt.nodes.new("ShaderNodeTexImage")
        n.image = img
        uv = nt.nodes.new("ShaderNodeUVMap")
        uv.uv_map = "UVMap"
        nt.links.new(uv.outputs["UV"], n.inputs["Vector"])
        return n

    base = tex(images["basecolor"])
    nt.links.new(base.outputs["Color"], bsdf.inputs["Base Color"])

    orm = tex(images["orm"])
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(orm.outputs["Color"], sep.inputs["Color"])
    nt.links.new(sep.outputs["Green"], bsdf.inputs["Roughness"])
    nt.links.new(sep.outputs["Blue"], bsdf.inputs["Metallic"])
    occ = nt.nodes.new("ShaderNodeGroup")
    occ.node_tree = gltf_output_group()
    nt.links.new(sep.outputs["Red"], occ.inputs["Occlusion"])

    nrm = tex(images["normal"])
    nmap = nt.nodes.new("ShaderNodeNormalMap")
    nmap.uv_map = "UVMap"
    nt.links.new(nrm.outputs["Color"], nmap.inputs["Color"])
    nt.links.new(nmap.outputs["Normal"], bsdf.inputs["Normal"])
    return m


def strip_bake_data(obj):
    me = obj.data
    if "detail_uv" in me.uv_layers:
        me.uv_layers.remove(me.uv_layers["detail_uv"])
    for a in [a for a in me.attributes if not a.name.startswith((".", "UVMap", "position", "sharp_"))
              and a.name not in ("material_index",) and a.domain == "CORNER" and a.data_type == "FLOAT"]:
        me.attributes.remove(a)


def make_lods(lod0, ratios):
    lods = [lod0]
    for i, r in enumerate(ratios, start=1):
        o = lod0.copy()
        o.data = lod0.data.copy()
        o.name = lod0.name.replace("LOD0", f"LOD{i}")
        o.data.name = o.name
        bpy.context.scene.collection.objects.link(o)
        d = o.modifiers.new("Decimate", "DECIMATE")
        d.decimate_type = "COLLAPSE"
        d.ratio = r
        d.use_collapse_triangulate = True
        bpy.context.view_layer.objects.active = o
        bpy.ops.object.modifier_apply(modifier=d.name)
        lods.append(o)
    return lods


def triangulate(obj):
    t = obj.modifiers.new("Triangulate", "TRIANGULATE")
    t.keep_custom_normals = True
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_apply(modifier=t.name)


def export_glb(objs, path):
    """Export `objs` to `path`. A `.glb` embeds everything; a `.gltf` (used for map props) writes
    its geometry next to it and its textures to the shared `../textures`, named by content, so
    models built from the same texture load (and upload to the GPU) one copy of it."""
    if path.endswith(".gltf"):
        return _export_gltf_shared(objs, path)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.export_scene.gltf(
        filepath=path,
        export_format="GLB",
        use_selection=True,
        export_yup=True,
        export_apply=True,
        export_tangents=True,
        export_normals=True,
        export_materials="EXPORT",
        export_image_format="AUTO",
    )
    return {o.name: tri_count(o) for o in objs}


def _export_gltf_shared(objs, path):
    out_dir = os.path.dirname(path)
    tex_dir = os.path.join(os.path.dirname(out_dir), "textures")
    os.makedirs(tex_dir, exist_ok=True)
    stem = os.path.splitext(os.path.basename(path))[0]
    tmp = tempfile.mkdtemp(prefix="vivad_gltf_")
    try:
        bpy.ops.object.select_all(action="DESELECT")
        for o in objs:
            o.select_set(True)
        bpy.context.view_layer.objects.active = objs[0]
        bpy.ops.export_scene.gltf(
            filepath=os.path.join(tmp, f"{stem}.gltf"),
            export_format="GLTF_SEPARATE",
            use_selection=True,
            export_yup=True,
            export_apply=True,
            export_tangents=True,
            export_normals=True,
            export_materials="EXPORT",
            export_image_format="AUTO",
        )
        with open(os.path.join(tmp, f"{stem}.gltf")) as f:
            doc = json.load(f)
        for im in doc.get("images", []):
            src = os.path.join(tmp, im["uri"])
            with open(src, "rb") as f:
                digest = hashlib.sha1(f.read()).hexdigest()[:10]
            base, ext = os.path.splitext(os.path.basename(im["uri"]))
            name = f"{base}_{digest}{ext}"
            if not os.path.exists(os.path.join(tex_dir, name)):
                shutil.copyfile(src, os.path.join(tex_dir, name))
            im["uri"] = f"../textures/{name}"
        for buf in doc.get("buffers", []):
            shutil.copyfile(os.path.join(tmp, buf["uri"]), os.path.join(out_dir, f"{stem}.bin"))
            buf["uri"] = f"{stem}.bin"
        with open(path, "w") as f:
            json.dump(doc, f, separators=(",", ":"))
        old = os.path.splitext(path)[0] + ".glb"
        if os.path.exists(old):
            os.remove(old)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    return {o.name: tri_count(o) for o in objs}

