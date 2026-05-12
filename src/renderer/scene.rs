use super::mesh::Mesh;

#[derive(Debug)]
pub struct Scene {
    clear_color: wgpu::Color,
    meshes: Vec<Mesh>,
}

impl Scene {
    pub fn clear_color(&self) -> wgpu::Color {
        self.clear_color
    }

    pub fn meshes(&self) -> &[Mesh] {
        &self.meshes
    }
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            clear_color: wgpu::Color {
                r: 0.53,
                g: 0.81,
                b: 0.92,
                a: 1.0,
            },
            meshes: Vec::new(),
        }
    }
}
