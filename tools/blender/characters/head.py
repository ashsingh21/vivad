"""The head and eyes at close-up quality, between the proxy and the bake.

subdivide(): one Catmull-Clark level on the head. The first design (shrinkwrap onto the basemesh)
is gone: the head is a closed surface running into the mouth and sockets, so shrinkwrap pulled lips
and lids onto the wrong sheet, and one level is 31k triangles against a 15-20k budget. Instead the
cage is pre-corrected so the smooth surface still passes through the proxy's own vertices, the neck
ring is pinned, the body gets matching vertices on its side of the ring, and finish.skin_lod takes
the scalp and neck back down to budget with the face held (`detail`, face.regions).

eye_parts(): the MPFB high-poly eye is a ball with the iris and a transparent shell, both whole
spheres. Each is cut to the part that can show: ball + iris (baked) and a cornea cap (unbaked, Blend).

Proxy and eye meshes face -Y in Blender (MakeHuman), +X is the character's left.
"""

import bmesh
import bpy
import numpy as np


def positions(me):
    co = np.empty(len(me.vertices) * 3, dtype=np.float64)
    me.vertices.foreach_get("co", co)
    return co.reshape(-1, 3)


def key(co):
    return tuple(round(x, 5) for x in co)


def open_ring(me):
    """Rounded positions of the mesh's open edge: on the cut head, the neck ring and nothing else."""
    bm = bmesh.new()
    bm.from_mesh(me)
    ring = {key(v.co) for v in bm.verts if v.is_boundary}
    bm.free()
    return ring


def split_ring_edges(body, ring):
    """Give the body the midpoints the head's ring gains, so the two still weld vertex to vertex."""
    bm = bmesh.new()
    bm.from_mesh(body.data)
    edges = [e for e in bm.edges if e.is_boundary and all(key(v.co) in ring for v in e.verts)]
    bmesh.ops.subdivide_edges(bm, edges=edges, cuts=1, use_grid_fill=False)
    bm.to_mesh(body.data)
    bm.free()
    return len(edges)


def subdivided(obj, count):
    """The first `count` vertices of `obj` as its modifiers leave it: the cage's own, on the surface."""
    e = obj.evaluated_get(bpy.context.evaluated_depsgraph_get())
    me = e.to_mesh()
    out = positions(me)[:count]
    e.to_mesh_clear()
    return out


def subdivide(head, body, sharpen=2):
    """One level on `head`, whose open edge is the neck; the body's side of the ring is split to
    match. `sharpen` passes move the cage so the surface doesn't shrink off the lips, nose and lids."""
    me = head.data
    ring = open_ring(me)
    original = positions(me)
    pinned = np.array([key(c) in ring for c in original])
    crease = me.attributes.new("crease_vert", "FLOAT", "POINT")
    crease.data.foreach_set("value", pinned.astype(np.float32))
    mod = head.modifiers.new("Subsurf", "SUBSURF")
    mod.levels = mod.render_levels = 1
    mod.boundary_smooth = "PRESERVE_CORNERS"
    bpy.context.view_layer.objects.active = head
    bpy.ops.object.modifier_move_to_index(modifier=mod.name, index=0)
    cage = original.copy()
    drift = 0.0
    for _ in range(sharpen + 1):
        me.vertices.foreach_set("co", cage.ravel())
        me.update()
        off = original - subdivided(head, len(cage))
        drift = float(np.linalg.norm(off, axis=1).max())
        print(f"[head] surface off the proxy by at most {drift * 1000:.2f} mm")
        cage += off
    if drift > 2e-3:
        raise RuntimeError("the subdivided cage is not in the proxy's vertex order")
    cage -= off
    me.vertices.foreach_set("co", cage.ravel())
    me.update()
    bpy.ops.object.modifier_apply(modifier=mod.name)
    if "crease_vert" in me.attributes:
        me.attributes.remove(me.attributes["crease_vert"])
    split = split_ring_edges(body, ring)
    print(f"[head] {len(original)} -> {len(me.vertices)} vertices, {split} ring edges split on the body")


def islands(bm):
    todo = set(bm.verts)
    while todo:
        stack = [todo.pop()]
        piece = []
        while stack:
            v = stack.pop()
            piece.append(v)
            for e in v.link_edges:
                w = e.other_vert(v)
                if w in todo:
                    todo.remove(w)
                    stack.append(w)
        yield piece


def keep_faces(obj, keep):
    """Delete the faces of `obj` that `keep` (face -> bool) refuses."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if not keep(f)], context="FACES")
    bm.to_mesh(obj.data)
    bm.free()


def mid_height(obj):
    """The object's vertical middle, in object space."""
    z = positions(obj.data)[:, 2]
    return (z.min() + z.max()) / 2


def eye_parts(eyes, name):
    """Split the high-poly eyes into (ball with iris, cornea caps). The ball keeps what faces out of
    the socket, so its UV islands pack larger in the small atlas."""
    bm = bmesh.new()
    bm.from_mesh(eyes.data)
    sides = {}
    for piece in islands(bm):
        sides.setdefault(piece[0].co.x > 0, []).append(piece)
    shell_verts = set()
    axis = {}
    for side, pieces in sides.items():
        # The shell is the piece that reaches furthest forward.
        shell = min(pieces, key=lambda p: min(v.co.y for v in p))
        shell_verts.update(v.index for v in shell)
        ys = [v.co.y for v in shell]
        axis[side] = ((min(ys) + max(ys)) / 2, (max(ys) - min(ys)) / 2)
    bm.free()

    def in_front(f, reach):
        centre, radius = axis[f.verts[0].co.x > 0]
        return sum(v.co.y for v in f.verts) / len(f.verts) < centre - reach * radius

    cornea = eyes.copy()
    cornea.data = eyes.data.copy()
    cornea.name = cornea.data.name = name
    bpy.context.scene.collection.objects.link(cornea)
    keep_faces(cornea, lambda f: f.verts[0].index in shell_verts and in_front(f, 0.25))
    keep_faces(eyes, lambda f: f.verts[0].index not in shell_verts and in_front(f, -0.15))
    return eyes, cornea
