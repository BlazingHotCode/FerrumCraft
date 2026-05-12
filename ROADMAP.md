# FerrumCraft Roadmap

This roadmap tracks broad implementation milestones. Checked items are complete
enough to support the next phase, but may still be refined as nearby systems are
expanded.

## Phase 1 — Foundation
- [x] Window with winit
- [x] wgpu rendering pipeline (clear screen)
- [x] Renderer architecture cleanup (separate scene, mesh, and pipeline responsibilities)
- [x] Basic static 3D renderer (vertex/index buffers, depth testing, perspective projection)
- [x] Built-in debug shapes (triangle, cube, plane)
- [ ] Event loop integration (resize, redraw, input plumbing)
- [ ] Basic shaders + material/color pipeline

## Phase 2 — Camera & Controls
- [ ] First-person camera (view/projection matrices)
- [ ] Mouse look (pointer lock)
- [ ] WASD movement + sprint
- [ ] Block highlighting (raycast against air)

## Phase 3 — World Data
- [ ] Chunk storage (16×64×16 blocks)
- [ ] Block registry (IDs, names, properties)
- [ ] World struct (collection of chunks)

## Phase 4 — Rendering Blocks
- [ ] Naive meshing (6 quads per block)
- [ ] Atlas or individual textures
- [ ] Frustum culling (don't draw chunks outside view)
- [ ] Opaque/transparent render passes

## Phase 5 — Terrain
- [ ] Heightmap / noise-based worldgen
- [ ] Block placement (grass, dirt, stone, wood, leaves)
- [ ] Loading chunks around player (load/unload distance)

## Phase 6 — Interaction
- [ ] Block breaking (hold-click progress)
- [ ] Block placing (crosshair target + adjacent face)
- [ ] Drop/pick up items (basic entity)

## Phase 7 — Gameplay
- [ ] Inventory system (hotbar + screen)
- [ ] Crafting table UI (2×2 → 3×3 grid)
- [ ] Lighting (sky light + block light, flood-fill)

## Phase 8 — Polish
- [ ] Skybox / fog
- [ ] Audio (steps, dig, place, ambient)
- [ ] Day/night cycle
- [ ] Save/load world to disk
- [ ] Main menu + pause screen

## Phase 9 — Stretch
- [ ] Physics (gravity, water, falling blocks)
- [ ] Entities (mobs, item drops)
- [ ] Multiplayer (basic client-server)

---

*Inspired by Minecraft — built in Rust.*
