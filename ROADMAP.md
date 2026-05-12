# FerrumCraft Roadmap

FerrumCraft is a Minecraft-inspired voxel survival sandbox built in Rust. This
roadmap prioritizes the core loop first: explore a block world, mine and place
blocks, collect resources, craft items, survive, and persist the world.

Checked items are complete enough to support the next phase, but may still be
refined as adjacent systems mature.

## Phase 1 - Engine Foundation
- [x] Window with winit
- [x] wgpu rendering pipeline (clear screen)
- [x] Renderer architecture cleanup (separate scene, mesh, and pipeline responsibilities)
- [x] Basic static 3D renderer (vertex/index buffers, depth testing, perspective projection)
- [x] Built-in debug shapes (triangle, cube, plane)
- [x] Event loop integration (resize, redraw, input plumbing)
- [x] Basic shaders + material/color pipeline
- [x] Fixed timestep game update loop
- [ ] Debug overlay (FPS, frame time, player/chunk position)

## Phase 2 - First-Person Player
- [ ] First-person camera with view/projection matrices
- [ ] Mouse look with pointer lock
- [ ] WASD movement, jump, crouch, and sprint
- [ ] Player collision capsule/AABB against blocks
- [ ] Gravity and grounded/falling state
- [ ] Reach raycast from camera center
- [ ] Crosshair and block face targeting

## Phase 3 - Blocks And World Data
- [ ] Block registry with numeric IDs, names, and properties
- [ ] Core blocks: air, grass, dirt, stone, sand, water, log, leaves, planks, glass
- [ ] Block properties: solid, opaque, transparent, liquid, hardness, drops
- [ ] Chunk storage (16x64x16 initially, expandable later)
- [ ] World struct managing loaded chunks by chunk coordinate
- [ ] Safe block get/set APIs across chunk boundaries
- [ ] Dirty chunk tracking for remeshing and saving

## Phase 4 - Chunk Rendering
- [ ] Naive block meshing (visible faces only)
- [ ] Chunk vertex/index buffers
- [ ] Texture atlas and UV generation
- [ ] Basic block material/color mapping
- [ ] Depth-tested opaque chunk pass
- [ ] Transparent block pass for water/glass/leaves
- [ ] Frustum culling for chunks outside view
- [ ] Simple chunk render distance control

## Phase 5 - Terrain Generation
- [ ] Deterministic world seed
- [ ] Heightmap/noise terrain generation
- [ ] Surface layering (grass, dirt, stone)
- [ ] Sand near water and simple beaches
- [ ] Trees with logs and leaves
- [ ] Basic caves or ore pockets
- [ ] Spawn area generation around player
- [ ] Async/incremental chunk loading around player
- [ ] Chunk unload distance

## Phase 6 - Block Interaction
- [ ] Block highlighting outline on targeted face
- [ ] Block breaking with hold-click progress and hardness
- [ ] Block placement on adjacent targeted face
- [ ] Collision checks that prevent placing blocks inside player
- [ ] Block drops as item entities
- [ ] Item pickup into inventory
- [ ] Hotbar selection affects placed block/item

## Phase 7 - Inventory And Crafting
- [ ] Item registry linked to block drops and placeable blocks
- [ ] Player inventory slots
- [ ] Hotbar UI and selection controls
- [ ] Inventory screen UI
- [ ] Crafting recipes data model
- [ ] 2x2 player crafting grid
- [ ] Crafting table block
- [ ] 3x3 crafting table UI
- [ ] Basic recipes: planks, sticks, crafting table, tools

## Phase 8 - Lighting And Atmosphere
- [ ] Sky light data per block
- [ ] Block light data per block
- [ ] Flood-fill light propagation
- [ ] Chunk boundary light propagation
- [ ] Vertex light values in chunk meshes
- [ ] Day/night cycle
- [ ] Sky color, fog, and sun/moon direction
- [ ] Torch block emitting light

## Phase 9 - Survival Loop
- [ ] Health system
- [ ] Hunger/stamina system
- [ ] Damage sources (fall damage, drowning, mobs later)
- [ ] Tool tiers and mining speed modifiers
- [ ] Durability for tools
- [ ] Simple resource progression (wood to stone tools)
- [ ] Furnace block and smelting recipes
- [ ] Basic death/respawn flow

## Phase 10 - Persistence
- [ ] Save directory and world metadata
- [ ] Serialize player state and inventory
- [ ] Serialize modified chunks
- [ ] Load existing worlds from disk
- [ ] Save dirty chunks incrementally
- [ ] Main menu with create/load world
- [ ] Pause menu with save and quit

## Phase 11 - Entities And Mobs
- [ ] Entity system with transforms and velocity
- [ ] Item drop entities
- [ ] Simple passive mob
- [ ] Simple hostile mob
- [ ] Entity collision against blocks
- [ ] Basic mob AI: wander, chase, attack
- [ ] Entity spawning rules by biome/light level

## Phase 12 - Biomes And World Variety
- [ ] Biome map generation
- [ ] Plains, forest, desert, and hills biomes
- [ ] Biome-specific surface blocks and trees
- [ ] Ores by depth bands
- [ ] Water bodies and rivers
- [ ] Structure hook for simple generated features

## Phase 13 - Polish
- [ ] Footstep, dig, place, pickup, and ambient audio
- [ ] Block particles for breaking
- [ ] Hand/item view model
- [ ] Settings menu (render distance, sensitivity, volume)
- [ ] Keybind configuration
- [ ] Screenshot/debug capture tools
- [ ] Packaging/release build workflow

## Phase 14 - Stretch Goals
- [ ] Infinite vertical chunks or taller worlds
- [ ] Multiplayer client/server prototype
- [ ] Redstone-like block logic
- [ ] Fluids with simple flow simulation
- [ ] Mod/data-pack loading for blocks, items, and recipes

---

*Inspired by Minecraft, but implemented with FerrumCraft's own engine and systems.*
