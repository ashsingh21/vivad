# NCR map: what we reuse (1 Nov 2026)

The online search was stopped early at the owner's request. These picks come from the search
reports. Each report's licence and version facts were read by its author, but the independent
second check did not finish, and the 3D model search returned nothing. Check a licence and version
again before adopting anything not yet in the repo.

## In use now
- **OpenStreetMap through Geofabrik.** Northern Zone holds Delhi and Gurugram, Central Zone holds
  Noida and Ghaziabad. ODbL 1.0. This replaced the slow public Overpass servers.
- **pyosmium** reads those files offline (`tools/osm/ncr_extract.py`).
- **Bevy's own `VisibilityRange`** for distance tiers, as the repo already uses it.

## Next, in order
| Need | Use | Licence | Why |
|---|---|---|---|
| Building heights | Google Open Buildings 2.5D Temporal | CC BY 4.0 or ODbL | The only usable height data here: OSM has heights on under 5 % of buildings. Replaces today's guessed ranges. Caps at 100 m, so set taller towers by hand |
| Ground type | ESA WorldCover 2021 | CC BY 4.0 | Tells parks, scrub, bare land and built land apart for the open land |
| Terrain | Copernicus GLO-30 | Copernicus licence, credit needed | Only if real relief is wanted. The land is nearly flat, so this is low priority |
| River edges | JRC Global Surface Water | CC BY 4.0 | Real Yamuna banks and sand bars |
| Bot navigation | `polyanya` crate | MIT or Apache-2.0 | Any-angle paths on a polygon mesh, with layers for floors and bridges. Fits glam 0.33, no Bevy dependency |
| Terrain mesh | Clipmap code from `jackdaw_terrain` | MIT or Apache-2.0 | About 34k vertices against about 6M. Early-stage crate, so pin it or copy the two files with the licence notice |
| Road surfaces | `osm2streets`, offline | Apache-2.0 | Lanes and junction shapes. Clean on Connaught Place, with some junction errors |

## Tried to avoid
- **Collider streaming:** every peer runs the whole simulation, so only visuals can stream.
- **Earth2Road:** the best road and junction tool found, MIT, but five weeks old and untested on
  macOS. Worth a trial only if road looks become the bottleneck.
- **Bevy meshlets:** no WebGPU support, which rules out the browser build.

## Must be written by us
Nothing ready-made fits the minimap, far meshes on Bevy 0.20, ziplines or the metro.

## Duties
Credit OpenStreetMap, ODbL 1.0, in the game. The converted `ncr_*.json` files are share-alike
under ODbL. Each extra data source above adds its own credit line.
