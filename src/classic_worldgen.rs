use crate::block::BlockId;
use crate::world::{CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z, Chunk, ChunkPos};

const AIR: u8 = 0;
const STONE: u8 = 1;
const GRASS: u8 = 2;
const DIRT: u8 = 3;
const WATER: u8 = 9;
const LAVA: u8 = 11;
const SAND: u8 = 12;
const GRAVEL: u8 = 13;
const GOLD_ORE: u8 = 14;
const IRON_ORE: u8 = 15;
const COAL_ORE: u8 = 16;
const LOG: u8 = 17;
const LEAVES: u8 = 18;

/// A complete finite Early Classic level generated in the archived stage order.
pub struct ClassicTerrain {
    width: usize,
    length: usize,
    blocks: Box<[u8]>,
    spawn: [i32; 3],
}

impl ClassicTerrain {
    pub fn generate(seed: u64, width: usize, length: usize) -> Self {
        let mut generator = Generator::new(seed, width, length);
        generator.generate();
        let spawn = generator.find_spawn(seed ^ 0x5DEECE66D);
        Self {
            width,
            length,
            blocks: generator.blocks.into_boxed_slice(),
            spawn,
        }
    }

    pub fn spawn(&self) -> [i32; 3] {
        self.spawn
    }

    pub fn chunk(&self, pos: ChunkPos) -> Option<Chunk> {
        let min_x = pos.0 * CHUNK_SIZE_X as i32;
        let min_z = pos.1 * CHUNK_SIZE_Z as i32;
        if min_x < 0 || min_z < 0 || min_x as usize >= self.width || min_z as usize >= self.length {
            return None;
        }

        let mut chunk = Chunk::new(pos);
        for y in 0..CHUNK_SIZE_Y {
            for z in 0..CHUNK_SIZE_Z {
                for x in 0..CHUNK_SIZE_X {
                    let world_x = min_x as usize + x;
                    let world_z = min_z as usize + z;
                    let block = self.blocks[self.index(world_x, y, world_z)];
                    chunk.set_block(x, y, z, block_id(block));
                }
            }
        }
        chunk.clear_dirty();
        Some(chunk)
    }

    fn index(&self, x: usize, y: usize, z: usize) -> usize {
        (y * self.length + z) * self.width + x
    }
}

fn block_id(id: u8) -> BlockId {
    BlockId(
        match id {
            STONE => "stone",
            GRASS => "grass_block",
            DIRT => "dirt",
            WATER => "water",
            LAVA => "lava",
            SAND => "sand",
            GRAVEL => "gravel",
            GOLD_ORE => "gold_ore",
            IRON_ORE => "iron_ore",
            COAL_ORE => "coal_ore",
            LOG => "oak_log",
            LEAVES => "oak_leaves",
            _ => "air",
        }
        .to_string(),
    )
}

struct Generator {
    width: usize,
    length: usize,
    blocks: Vec<u8>,
    heights: Vec<i32>,
    random: JavaRandom,
}

impl Generator {
    fn new(seed: u64, width: usize, length: usize) -> Self {
        Self {
            width,
            length,
            blocks: vec![AIR; width * length * CHUNK_SIZE_Y],
            heights: vec![0; width * length],
            random: JavaRandom::new(seed),
        }
    }

    fn generate(&mut self) {
        self.raise();
        self.erode();
        self.soil();
        self.carve_caves();
        self.carve_ore(COAL_ORE, 90);
        self.carve_ore(IRON_ORE, 70);
        self.carve_ore(GOLD_ORE, 50);
        self.water();
        self.lava();
        self.grow();
        self.plant_trees();
    }

    fn raise(&mut self) {
        let first = DistortedNoise::new(&mut self.random);
        let second = DistortedNoise::new(&mut self.random);
        let selector = OctaveNoise::new(&mut self.random);
        for x in 0..self.width {
            for z in 0..self.length {
                let xz = x as f32 * 1.3;
                let zz = z as f32 * 1.3;
                let first_height = first.sample(xz as f64, zz as f64) / 8.0 - 8.0;
                let mut second_height = second.sample(xz as f64, zz as f64) / 6.0 + 6.0;
                if selector.sample(x as f64, z as f64) / 8.0 > 0.0 {
                    second_height = first_height;
                }
                let mut height = first_height.max(second_height) / 2.0;
                if height < 0.0 {
                    height /= 2.0;
                }
                self.heights[x + z * self.width] = height as i32;
            }
        }
    }

    fn erode(&mut self) {
        let erosion = DistortedNoise::new(&mut self.random);
        let selector = DistortedNoise::new(&mut self.random);
        for x in 0..self.width {
            for z in 0..self.length {
                let amount = erosion.sample((x << 1) as f64, (z << 1) as f64) / 8.0;
                let parity = i32::from(selector.sample((x << 1) as f64, (z << 1) as f64) > 0.0);
                if amount > 2.0 {
                    let index = x + z * self.width;
                    self.heights[index] = ((self.heights[index] - parity) / 2) * 2 + parity;
                }
            }
        }
    }

    fn soil(&mut self) {
        let soil = OctaveNoise::new(&mut self.random);
        for x in 0..self.width {
            for z in 0..self.length {
                let offset = (soil.sample(x as f64, z as f64) / 24.0) as i32 - 4;
                let dirt_top = self.heights[x + z * self.width] + 32;
                let stone_top = dirt_top + offset;
                self.heights[x + z * self.width] = dirt_top.max(stone_top);
                for y in 0..CHUNK_SIZE_Y as i32 {
                    let block = if y <= stone_top {
                        STONE
                    } else if y <= dirt_top {
                        DIRT
                    } else {
                        AIR
                    };
                    let index = self.index(x, y as usize, z);
                    self.blocks[index] = block;
                }
            }
        }
    }

    fn carve_caves(&mut self) {
        let count = self.width * self.length * CHUNK_SIZE_Y / 256 / 64;
        for _ in 0..count {
            let x = self.random.next_float() * self.width as f32;
            let y = self.random.next_float() * CHUNK_SIZE_Y as f32;
            let z = self.random.next_float() * self.length as f32;
            let length = ((self.random.next_float() + self.random.next_float()) * 75.0) as i32;
            self.carve_path(x, y, z, length, 2.5, AIR);
        }
    }

    fn carve_ore(&mut self, ore: u8, abundance: i32) {
        let count = self.width * self.length * CHUNK_SIZE_Y / 256 / 64 * abundance as usize / 100;
        for _ in 0..count {
            let x = self.random.next_float() * self.width as f32;
            let y = self.random.next_float() * CHUNK_SIZE_Y as f32;
            let z = self.random.next_float() * self.length as f32;
            let length =
                ((self.random.next_float() + self.random.next_float()) * 75.0 * abundance as f32
                    / 100.0) as i32;
            self.carve_path(x, y, z, length, abundance as f32 / 100.0, ore);
        }
    }

    fn carve_path(
        &mut self,
        mut x: f32,
        mut y: f32,
        mut z: f32,
        length: i32,
        radius_scale: f32,
        replacement: u8,
    ) {
        if length <= 0 {
            return;
        }
        let mut yaw = (self.random.next_float() as f64 * std::f64::consts::TAU) as f32;
        let mut yaw_velocity = 0.0f32;
        let mut pitch = (self.random.next_float() as f64 * std::f64::consts::TAU) as f32;
        let mut pitch_velocity = 0.0f32;
        for step in 0..length {
            x = (x as f64 + (yaw as f64).sin() * (pitch as f64).cos()) as f32;
            z = (z as f64 + (yaw as f64).cos() * (pitch as f64).cos()) as f32;
            y = (y as f64 + (pitch as f64).sin()) as f32;
            yaw += yaw_velocity * 0.2;
            yaw_velocity *= 0.9;
            yaw_velocity += self.random.next_float() - self.random.next_float();
            pitch += pitch_velocity * 0.5;
            pitch *= 0.5;
            pitch_velocity *= 0.9;
            pitch_velocity += self.random.next_float() - self.random.next_float();
            let radius = ((step as f64 * std::f64::consts::PI / length as f64).sin()
                * radius_scale as f64
                + 1.0) as f32;
            self.carve_ellipsoid(x, y, z, radius, replacement);
        }
    }

    fn carve_ellipsoid(&mut self, x: f32, y: f32, z: f32, radius: f32, replacement: u8) {
        let min_x = (x - radius) as i32;
        let max_x = (x + radius) as i32;
        let min_y = (y - radius) as i32;
        let max_y = (y + radius) as i32;
        let min_z = (z - radius) as i32;
        let max_z = (z + radius) as i32;
        for bx in min_x..=max_x {
            for by in min_y..=max_y {
                for bz in min_z..=max_z {
                    if bx < 1
                        || by < 1
                        || bz < 1
                        || bx >= self.width as i32 - 1
                        || by >= CHUNK_SIZE_Y as i32 - 1
                        || bz >= self.length as i32 - 1
                    {
                        continue;
                    }
                    let dx = bx as f32 - x;
                    let dy = by as f32 - y;
                    let dz = bz as f32 - z;
                    if dx * dx + dy * dy * 2.0 + dz * dz >= radius * radius {
                        continue;
                    }
                    let index = self.index(bx as usize, by as usize, bz as usize);
                    if self.blocks[index] == STONE {
                        self.blocks[index] = replacement;
                    }
                }
            }
        }
    }

    fn water(&mut self) {
        let y = CHUNK_SIZE_Y / 2 - 1;
        for x in 0..self.width {
            self.flood(x, y, 0, WATER);
            self.flood(x, y, self.length - 1, WATER);
        }
        for z in 0..self.length {
            self.flood(0, y, z, WATER);
            self.flood(self.width - 1, y, z, WATER);
        }
        for _ in 0..self.width * self.length / 200 {
            let x = self.random.next_int(self.width as i32) as usize;
            let y = y - self.random.next_int(3) as usize;
            let z = self.random.next_int(self.length as i32) as usize;
            self.flood(x, y, z, WATER);
        }
    }

    fn lava(&mut self) {
        for _ in 0..self.width * self.length * CHUNK_SIZE_Y / 10_000 {
            let x = self.random.next_int(self.width as i32) as usize;
            let y = self.random.next_int((CHUNK_SIZE_Y / 2 - 4) as i32) as usize;
            let z = self.random.next_int(self.length as i32) as usize;
            self.flood(x, y, z, LAVA);
        }
    }

    fn flood(&mut self, x: usize, y: usize, z: usize, fluid: u8) {
        let start = self.index(x, y, z);
        if self.blocks[start] != AIR {
            return;
        }
        self.blocks[start] = fluid;
        let mut stack = vec![(x, y, z)];
        while let Some((x, y, z)) = stack.pop() {
            for (nx, nz) in [
                (x.wrapping_sub(1), z),
                (x + 1, z),
                (x, z.wrapping_sub(1)),
                (x, z + 1),
            ] {
                if nx >= self.width || nz >= self.length {
                    continue;
                }
                let index = self.index(nx, y, nz);
                if self.blocks[index] == AIR {
                    self.blocks[index] = fluid;
                    stack.push((nx, y, nz));
                }
            }
            if y == 0 {
                continue;
            }
            let below = self.index(x, y - 1, z);
            if fluid == LAVA && self.blocks[below] == WATER {
                self.blocks[below] = STONE;
            } else if self.blocks[below] == AIR {
                self.blocks[below] = fluid;
                stack.push((x, y - 1, z));
            }
        }
    }

    fn grow(&mut self) {
        let sand = OctaveNoise::new(&mut self.random);
        let gravel = OctaveNoise::new(&mut self.random);
        for x in 0..self.width {
            for z in 0..self.length {
                let sandy = sand.sample(x as f64, z as f64) > 8.0;
                let gravelly = gravel.sample(x as f64, z as f64) > 12.0;
                let y = self.heights[x + z * self.width];
                if !(0..CHUNK_SIZE_Y as i32 - 1).contains(&y) {
                    continue;
                }
                if self.blocks[self.index(x, y as usize + 1, z)] != AIR {
                    continue;
                }
                let surface = if y <= 31 && sandy {
                    SAND
                } else if y <= 31 && gravelly {
                    GRAVEL
                } else {
                    GRASS
                };
                let index = self.index(x, y as usize, z);
                self.blocks[index] = surface;
            }
        }
    }

    fn plant_trees(&mut self) {
        for _ in 0..self.width * self.length / 4000 {
            let origin_x = self.random.next_int(self.width as i32);
            let origin_z = self.random.next_int(self.length as i32);
            for _ in 0..20 {
                let mut x = origin_x;
                let mut z = origin_z;
                for _ in 0..20 {
                    x += self.random.next_int(6) - self.random.next_int(6);
                    z += self.random.next_int(6) - self.random.next_int(6);
                    if x < 0 || z < 0 || x >= self.width as i32 || z >= self.length as i32 {
                        continue;
                    }
                    let base_y = self.heights[x as usize + z as usize * self.width] + 1;
                    let height = self.random.next_int(2) + 4;
                    if self.can_place_tree(x, base_y, z, height) {
                        self.place_tree(x, base_y, z, height);
                    }
                }
            }
        }
    }

    fn can_place_tree(&self, x: i32, base_y: i32, z: i32, height: i32) -> bool {
        for y in base_y..=base_y + height + 1 {
            let radius = if y >= base_y + height - 1 { 2 } else { 1 };
            for bx in x - radius..=x + radius {
                for bz in z - radius..=z + radius {
                    if bx < 0
                        || bz < 0
                        || y < 0
                        || bx >= self.width as i32
                        || bz >= self.length as i32
                        || y >= CHUNK_SIZE_Y as i32
                        || self.blocks[self.index(bx as usize, y as usize, bz as usize)] != AIR
                    {
                        return false;
                    }
                }
            }
        }
        if base_y <= 0 || base_y >= CHUNK_SIZE_Y as i32 - height - 1 {
            return false;
        }
        self.blocks[self.index(x as usize, base_y as usize - 1, z as usize)] == GRASS
    }

    fn place_tree(&mut self, x: i32, base_y: i32, z: i32, height: i32) {
        let ground = self.index(x as usize, base_y as usize - 1, z as usize);
        self.blocks[ground] = DIRT;
        for y in base_y + height - 2..=base_y + height {
            let layer = y - (base_y + height);
            for bx in x - 1..=x + 1 {
                for bz in z - 1..=z + 1 {
                    if layer == 0 && (bx - x).abs() == 1 && (bz - z).abs() == 1 {
                        continue;
                    }
                    let index = self.index(bx as usize, y as usize, bz as usize);
                    self.blocks[index] = LEAVES;
                }
            }
        }
        for y in base_y..base_y + height {
            let index = self.index(x as usize, y as usize, z as usize);
            self.blocks[index] = LOG;
        }
    }

    fn find_spawn(&self, seed: u64) -> [i32; 3] {
        let mut random = JavaRandom::new(seed);
        let mut spawn = [self.width as i32 / 2, -100, self.length as i32 / 2];
        for _ in 0..10_000 {
            let x = random.next_int(self.width as i32 / 2) + self.width as i32 / 4;
            let z = random.next_int(self.length as i32 / 2) + self.length as i32 / 4;
            let mut y = CHUNK_SIZE_Y as i32 - 1;
            while y > 0 {
                let block = self.blocks[self.index(x as usize, y as usize, z as usize)];
                if block != AIR && block != WATER && block != LAVA {
                    break;
                }
                y -= 1;
            }
            y += 1;
            spawn = [x, y, z];
            if y > CHUNK_SIZE_Y as i32 / 2 {
                break;
            }
        }
        spawn
    }

    fn index(&self, x: usize, y: usize, z: usize) -> usize {
        (y * self.length + z) * self.width + x
    }
}

struct DistortedNoise {
    source: OctaveNoise,
    distortion: OctaveNoise,
}

impl DistortedNoise {
    fn new(random: &mut JavaRandom) -> Self {
        Self {
            source: OctaveNoise::new(random),
            distortion: OctaveNoise::new(random),
        }
    }

    fn sample(&self, x: f64, z: f64) -> f64 {
        self.source.sample(x + self.distortion.sample(x, z), z)
    }
}

struct OctaveNoise {
    octaves: [ImprovedNoise; 8],
}

impl OctaveNoise {
    fn new(random: &mut JavaRandom) -> Self {
        Self {
            octaves: std::array::from_fn(|_| ImprovedNoise::new(random)),
        }
    }

    fn sample(&self, x: f64, z: f64) -> f64 {
        let mut value = 0.0;
        let mut scale = 1.0;
        for octave in &self.octaves {
            value += octave.sample(x / scale, z / scale) * scale;
            scale *= 2.0;
        }
        value
    }
}

struct ImprovedNoise {
    permutations: [usize; 512],
}

impl ImprovedNoise {
    fn new(random: &mut JavaRandom) -> Self {
        let mut permutations = [0; 512];
        for (index, permutation) in permutations[..256].iter_mut().enumerate() {
            *permutation = index;
        }
        for index in 0..256 {
            let swap = random.next_int((256 - index) as i32) as usize + index;
            permutations.swap(index, swap);
            permutations[index + 256] = permutations[index];
        }
        Self { permutations }
    }

    fn sample(&self, x: f64, y: f64) -> f64 {
        let xi = x.floor() as i32 & 255;
        let yi = y.floor() as i32 & 255;
        let xf = x - x.floor();
        let yf = y - y.floor();
        let u = fade(xf);
        let v = fade(yf);
        let a = self.permutations[xi as usize] + yi as usize;
        let aa = self.permutations[a];
        let ab = self.permutations[a + 1];
        let b = self.permutations[xi as usize + 1] + yi as usize;
        let ba = self.permutations[b];
        let bb = self.permutations[b + 1];
        lerp(
            v,
            lerp(
                u,
                gradient(self.permutations[aa], xf, yf, 0.0),
                gradient(self.permutations[ba], xf - 1.0, yf, 0.0),
            ),
            lerp(
                u,
                gradient(self.permutations[ab], xf, yf - 1.0, 0.0),
                gradient(self.permutations[bb], xf - 1.0, yf - 1.0, 0.0),
            ),
        )
    }
}

fn fade(value: f64) -> f64 {
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

fn lerp(amount: f64, start: f64, end: f64) -> f64 {
    start + amount * (end - start)
}

fn gradient(hash: usize, x: f64, y: f64, z: f64) -> f64 {
    let hash = hash & 15;
    let first = if hash < 8 { x } else { y };
    let second = if hash < 4 {
        y
    } else if hash == 12 || hash == 14 {
        x
    } else {
        z
    };
    (if hash & 1 == 0 { first } else { -first }) + if hash & 2 == 0 { second } else { -second }
}

struct JavaRandom {
    state: u64,
}

impl JavaRandom {
    const MULTIPLIER: u64 = 0x5DEECE66D;
    const ADDEND: u64 = 0xB;
    const MASK: u64 = (1 << 48) - 1;

    fn new(seed: u64) -> Self {
        Self {
            state: (seed ^ Self::MULTIPLIER) & Self::MASK,
        }
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.state = (self
            .state
            .wrapping_mul(Self::MULTIPLIER)
            .wrapping_add(Self::ADDEND))
            & Self::MASK;
        (self.state >> (48 - bits)) as i32
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        assert!(bound > 0);
        if bound & (bound - 1) == 0 {
            return ((bound as i64 * self.next(31) as i64) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let value = bits % bound;
            if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                return value;
            }
        }
    }

    fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1u32 << 24) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_random_matches_known_sequence() {
        let mut random = JavaRandom::new(0);
        assert_eq!(random.next_int(1000), 360);
        assert_eq!(random.next_int(1000), 948);
        assert_eq!(random.next_int(1000), 29);
    }

    #[test]
    fn classic_terrain_is_deterministic_and_extracts_chunks() {
        let first = ClassicTerrain::generate(12345, 32, 32);
        let second = ClassicTerrain::generate(12345, 32, 32);
        assert_eq!(first.blocks, second.blocks);
        assert!(first.chunk(ChunkPos(0, 0)).is_some());
        assert!(first.chunk(ChunkPos(-1, 0)).is_none());
    }
}
