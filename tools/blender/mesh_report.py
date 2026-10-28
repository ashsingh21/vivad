"""Text report on exported models, in place of preview renders for geometry sanity: triangle
counts per LOD, size, open edges, loose pieces, degenerate faces, materials and textures.

blender -b --factory-startup -P tools/blender/mesh_report.py -- <model.gltf|glb> [...]
"""

import os
import sys

import bmesh
import bpy


def report(path):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=path)
    print(f"== {os.path.basename(path)}")
    for o in sorted((o for o in bpy.context.scene.objects if o.type == "MESH"), key=lambda o: o.name):
        me = o.evaluated_get(bpy.context.evaluated_depsgraph_get()).data
        bm = bmesh.new()
        bm.from_mesh(me)
        bm.transform(o.matrix_world)
        # glTF splits vertices at UV and normal seams; weld them so edges count truly.
        bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
        bm.verts.index_update()
        tris = sum(len(f.verts) - 2 for f in bm.faces)
        xs, ys, zs = zip(*(v.co[:] for v in bm.verts)) if bm.verts else ((0,), (0,), (0,))
        open_edges = sum(1 for e in bm.edges if len(e.link_faces) == 1)
        nonmanifold = sum(1 for e in bm.edges if len(e.link_faces) > 2)
        degenerate = sum(1 for f in bm.faces if f.calc_area() < 1e-8)
        # Loose pieces: connected vertex islands.
        seen, islands = set(), 0
        for v in bm.verts:
            if v.index in seen:
                continue
            islands += 1
            stack = [v]
            while stack:
                u = stack.pop()
                if u.index in seen:
                    continue
                seen.add(u.index)
                stack.extend(e.other_vert(u) for e in u.link_edges)
        mats = []
        for m in me.materials:
            if m is None:
                continue
            imgs = sorted({n.image.name for n in m.node_tree.nodes if n.type == "TEX_IMAGE" and n.image} if m.node_tree else [])
            mats.append(f"{m.name}({len(imgs)} tex){'' if m.use_backface_culling else ' 2-sided'}")
        print(f"  {o.name:<24} tris {tris:>7}  size {max(xs) - min(xs):6.2f} x {max(ys) - min(ys):6.2f} x {max(zs) - min(zs):6.2f} m"
              f"  z {min(zs):+.2f}..{max(zs):+.2f}  open-edges {open_edges}  non-manifold {nonmanifold}"
              f"  degenerate {degenerate}  pieces {islands}")
        print(f"  {'':<24} materials: {', '.join(mats) or '-'}")
        bm.free()


def main():
    for p in sys.argv[sys.argv.index("--") + 1:]:
        report(p)


if __name__ == "__main__":
    main()
