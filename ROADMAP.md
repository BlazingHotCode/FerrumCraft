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
- [x] Debug overlay (FPS, frame time, player/chunk position)
- [x] Structured logging system (log levels, categories, console/file output)

## Phase 2 - Free Camera Prototype
- [x] First-person camera with view/projection matrices
- [x] Mouse look with pointer lock
- [x] Free-fly world-axis movement (WASD horizontal, Space up, Shift down)

## Phase 3 - Data And Resource System
- [x] Namespaced string IDs (`ferrumcraft:stone`) for blocks, items, entities, recipes, loot tables, and tags
- [x] Asset/resource directory layout inspired by Minecraft (`assets/<namespace>/...`, `data/<namespace>/...`)
- [x] Resource loader for JSON files and textures
- [x] Registry bootstrap order for blocks, items, entities, biomes, features, sounds, particles, screens, commands, and dimensions
- [x] Language files for display names (`assets/<namespace>/lang/en_us.json`)
- [x] Block model JSON files (`assets/<namespace>/models/block/*.json`)
- [x] Item model JSON files (`assets/<namespace>/models/item/*.json`)
- [x] Blockstate JSON files for mapping block properties to models
- [x] Texture references in models (`assets/<namespace>/textures/...`)
- [x] JSON schema validation for every data/resource file type
- [x] Data validation with useful missing/invalid resource errors
- [x] Built-in `ferrumcraft` resource pack loaded by default
- [x] Stable save IDs use namespaced strings instead of runtime registry indexes

## Phase 4 - Blocks And World Data
- [x] Block registry populated from data/resource definitions
- [x] Block display names resolved through lang keys
- [x] Core blocks: air, grass, dirt, stone, sand, water, log, leaves, planks, glass
- [x] Block properties: solid, opaque, transparent, liquid, hardness, light emission
- [x] Data-driven block components (collision, flammable, gravity affected, inventory, smelting, etc.)
- [x] Blockstate property schemas defining allowed values (axis, facing, waterlogged, growth, etc.)
- [x] Blockstate storage for property variants (axis, facing, waterlogged, growth, etc.)
- [x] Chunk storage (16x64x16 initially, expandable later)
- [x] World struct managing loaded chunks by chunk coordinate
- [x] Safe block get/set APIs across chunk boundaries
- [x] Dirty chunk tracking for remeshing and saving

## Phase 5 - Chunk Rendering
- [ ] Naive block meshing (visible faces only)
- [ ] Chunk vertex/index buffers
- [ ] Texture atlas generated from model texture references
- [ ] UV generation from JSON block models
- [ ] Cube/block model renderer for parented models (e.g. `block/cube_all`, `block/cube_column`)
- [ ] Basic block material/color mapping from model/block data
- [ ] Depth-tested opaque chunk pass
- [ ] Transparent block pass for water/glass/leaves
- [ ] Frustum culling for chunks outside view
- [ ] Simple chunk render distance control

## Phase 6 - Terrain Generation
- [ ] Deterministic world seed
- [ ] Worldgen feature registry with namespaced string IDs
- [ ] Data-driven feature JSON for trees, ores, lakes, flowers, and structures
- [ ] Heightmap/noise terrain generation
- [ ] Surface layering (grass, dirt, stone)
- [ ] Sand near water and simple beaches
- [ ] Trees with logs and leaves
- [ ] Basic caves or ore pockets
- [ ] Configurable ore generation by depth bands and biome tags
- [ ] Spawn area generation around player
- [ ] Async/incremental chunk loading around player
- [ ] Chunk unload distance

## Phase 7 - First-Person Player
- [ ] Grounded WASD movement relative to camera yaw
- [ ] Jump, crouch, and sprint movement states
- [ ] Player collision capsule/AABB against blocks
- [ ] Gravity and grounded/falling state

## Phase 8 - Block Interaction
- [ ] Reach raycast from camera center
- [ ] Crosshair and block face targeting
- [ ] Block highlighting outline on targeted face
- [ ] Block breaking with hold-click progress and hardness
- [ ] Block placement on adjacent targeted face
- [ ] Collision checks that prevent placing blocks inside player
- [ ] Block drops from loot table JSON files (`data/<namespace>/loot_tables/blocks/*.json`)
- [ ] Item pickup into inventory
- [ ] Hotbar selection affects placed block/item

## Phase 9 - Inventory And Crafting
- [ ] Item registry populated from data/resource definitions
- [ ] Item display names resolved through lang keys
- [ ] Block items linked to block model/item model definitions
- [ ] Player inventory slots
- [ ] Hotbar UI and selection controls
- [ ] Inventory screen UI
- [ ] Crafting recipes loaded from JSON (`data/<namespace>/recipes/*.json`)
- [ ] Recipe types registry for shaped, shapeless, smelting, and future custom recipe types
- [ ] 2x2 player crafting grid
- [ ] Crafting table block
- [ ] 3x3 crafting table UI
- [ ] Basic recipes: planks, sticks, crafting table, tools

## Phase 10 - Lighting And Atmosphere
- [ ] Sky light data per block
- [ ] Block light data per block
- [ ] Flood-fill light propagation
- [ ] Chunk boundary light propagation
- [ ] Vertex light values in chunk meshes
- [ ] Day/night cycle
- [ ] Sky color, fog, and sun/moon direction
- [ ] Torch block emitting light
- [ ] Sound event registry for ambient and block-related sounds
- [ ] Particle registry for block breaking and ambient effects

## Phase 11 - Survival Loop
- [ ] Health system
- [ ] Hunger/stamina system
- [ ] Damage sources (fall damage, drowning, mobs later)
- [ ] Tool tiers and mining speed modifiers
- [ ] Durability for tools
- [ ] Simple resource progression (wood to stone tools)
- [ ] Furnace block and smelting recipes loaded from JSON
- [ ] Tool behavior driven by item components and tags
- [ ] Basic death/respawn flow

## Phase 12 - Persistence
- [ ] Save directory and world metadata
- [ ] Save game registry snapshot for detecting missing/renamed content
- [ ] Data migration system for renamed IDs and changed save schemas
- [ ] Serialize player state and inventory
- [ ] Serialize modified chunks
- [ ] Load existing worlds from disk
- [ ] Save dirty chunks incrementally
- [ ] App data directory structure (logs, saves, config, screenshots)
- [ ] User settings saved to config file in app data directory

## Phase 13 - UI And Menus
- [ ] Main menu screen (title, singleplayer, multiplayer placeholder, settings, quit)
- [ ] Title screen background / panorama
- [ ] Button navigation with keyboard and mouse
- [ ] Pause menu (resume, save and quit, settings, quit to title)
- [ ] Settings menu with categories (video, audio, controls)
- [ ] Render distance slider in video settings
- [ ] Mouse sensitivity slider in controls settings
- [ ] Volume sliders in audio settings
- [ ] Keybind/controls screen with rebindable actions
- [ ] World selection / load world screen
- [ ] New world creation screen (world name, seed)
- [ ] World deletion confirmation dialog
- [ ] Hotbar HUD element
- [ ] Crosshair HUD element
- [ ] Debug screen (F3) accessible from pause menu
- [ ] Settings persistence: save/load all settings to/from config file in app data
- [ ] Language selection in settings (loads different lang files)
- [ ] Accessibility options (UI scale, color blind mode placeholder)

## Phase 14 - Entities And Mobs
- [ ] Entity registry with namespaced string IDs
- [ ] Data-driven entity components (health, collision, gravity, AI goals, drops)
- [ ] Entity system with transforms and velocity
- [ ] Item drop entities
- [ ] Simple passive mob
- [ ] Simple hostile mob
- [ ] Entity collision against blocks
- [ ] Basic mob AI: wander, chase, attack
- [ ] Entity spawning rules by biome/light level

## Phase 14 - Biomes And World Variety
- [ ] Biome registry with namespaced string IDs
- [ ] Biome JSON definitions (temperature, humidity, terrain noise, surface blocks, features, mob spawns)
- [ ] Biome map generation
- [ ] Plains, forest, desert, and hills biomes
- [ ] Biome-specific surface blocks and trees
- [ ] Ores by depth bands
- [ ] Water bodies and rivers
- [ ] Structure hook for simple generated features
- [ ] Dimension registry and dimension JSON (height, sky/fog, gravity, worldgen preset)

## Phase 15 - Resource Packs And Extensibility
- [ ] External resource-pack loading with override order
- [ ] External data-pack loading with override order
- [ ] Reload command/hotkey for assets and data during development
- [ ] Missing-texture and missing-model fallbacks
- [ ] Namespaced dependency/conflict diagnostics
- [ ] Versioned pack metadata (`pack.json` or equivalent)
- [ ] Pack dependency and compatibility constraints
- [ ] Resource reload event that rebuilds registries, atlases, models, recipes, loot tables, tags, and lang cache safely
- [ ] Missing lang key fallback display (`namespace.path` style)
- [ ] Documented JSON schemas/examples for blocks, models, blockstates, items, recipes, loot tables, tags, biomes, and entities

## Phase 16 - Polish
- [ ] Footstep, dig, place, pickup, and ambient audio
- [ ] Block particles for breaking
- [ ] Hand/item view model
- [ ] Command system for debug/game commands
- [ ] Screenshot/debug capture tools
- [ ] Packaging/release build workflow

## Phase 17 - Stretch Goals
- [ ] Infinite vertical chunks or taller worlds
- [ ] Multiplayer client/server prototype
- [ ] Redstone-like block logic
- [ ] Fluids with simple flow simulation
- [ ] Plugin/mod API for native gameplay extensions

---

*Inspired by Minecraft, but implemented with FerrumCraft's own engine and systems.*
