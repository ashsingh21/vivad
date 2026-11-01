# NCR visual quality pass

The target is grounded contemporary NCR with mythic weapons. Human-scale details, restrained materials and believable daylight take precedence over adding more map area.

## Implemented

- Six original 512 px tiling PBR sets: asphalt aggregate, weathered concrete, staggered paving, ground grain, coated glass and brushed steel. Each has base colour, tangent-space normal and packed roughness/metalness data. Repeat sampling and mip chains keep distant surfaces stable.
- Fictional Niva Market shop signs and Blue Line entrance signs. Sign UVs cover each board rather than using the architecture's world-space tiling.
- A neutral daylight sun, cooler distance haze and a simple hemispherical environment light for glass/metal reflections. The camera keeps its existing aiming behaviour and antialiasing.
- An open-sided Pralaya cylinder: the old mesh's top cap was incorrectly tinting the whole map in overhead views.
- Paved building forecourts and sidewalks, kerbs and zebra crossings.
- Glazed ground floors, mullions, projecting cornices, window drip shades, apartment AC units and rooftop mechanical detail.
- Benches, bins, service cables, metro entrance boards, container locking bars and corrugation.
- Eleven static parked cars, each using one simple collider and visual body/tyre/window details. These are cover props, not drivable vehicles.
- Decorative distant buildings and ground beyond the arena boundary. They have no navigation or collision and are not additional playable districts.

## Reproduction and provenance

Run `just ncr-materials` to regenerate the texture set with `tools/materials/ncr_surfaces.py`. All surface maps are generated from seeded numeric patterns, not downloaded imagery. Signs use rasterized system-font text; no font file is bundled. Geometry is authored in `crates/shared/src/map/ncr_art.rs` and added to the existing map recipe. Existing trees, weapons, animation and character assets remain in use.

## Quality limits

This is an environmental realism pass on a procedural game, not a photorealistic asset set. The parked cars and architecture remain simplified meshes. Character outfits, detailed interiors, animated traffic, trains, full reflection probes and final foliage assets need further production work. The environment light approximates the sky and ground; it does not reflect nearby buildings or moving characters.

Final captures and validation results are recorded with the completed pass. Frame-rate samples from failed/black captures must not be used as performance evidence.
