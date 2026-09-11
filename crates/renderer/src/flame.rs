const GRID: f32 = 60.0;

#[derive(Clone, Copy, Default)]
struct Cell {
    key: [u32; 2],
    density: Option<f32>,
}

/// One animated flame draw. The cache never survives into the next frame.
pub struct Flame {
    time: f32,
    intensity: f32,
    primary: [f32; 3],
    secondary: [f32; 3],
    rise: f32,
    scale: f32,
    speed: f32,
    cache: bool,
    #[cfg(all(target_arch = "arm", feature = "flame-simd"))]
    simd: bool,
    cells: [Cell; 61],
}

impl Flame {
    pub fn new(
        time: f32,
        intensity: f32,
        id: f32,
        primary: [f32; 3],
        secondary: [f32; 3],
        cache: bool,
    ) -> Self {
        #[cfg(all(target_arch = "arm", feature = "flame-simd"))]
        static SIMD: std::sync::LazyLock<bool> =
            std::sync::LazyLock::new(|| std::env::var("BALATRO_FLAME_SIMD").as_deref() == Ok("1"));
        Self {
            time,
            intensity,
            primary,
            secondary,
            rise: (4.0 * time).rem_euclid(10000.0) - 5000.0 + (1.781 * id).rem_euclid(1000.0),
            scale: 7.5 + 3.0 / (2.0 + 2.0 * intensity),
            speed: (20.781 * id).rem_euclid(100.0) + (time + id).sin() * (time * 0.151 + id).cos(),
            cache,
            #[cfg(all(target_arch = "arm", feature = "flame-simd"))]
            simd: *SIMD,
            cells: [Cell::default(); 61],
        }
    }

    pub fn pixel(&mut self, ux: f32, uy: f32) -> [u8; 4] {
        let qx = (ux * GRID).floor();
        let qy = (uy * GRID).floor();
        let mut smoke = self.cell_density(qx, qy);

        // Only turbulence uses the shader's grid. Edges and colour use the
        // original pixel coordinates, even when they share a cached density.
        if ux.abs() > 0.4 {
            smoke += 10.0 * (ux.abs() - 0.4);
        }
        let adj_x = ux * 0.19;
        let adj_y = uy - 0.1;
        let len_adj = (adj_x * adj_x + adj_y * adj_y).sqrt();
        if len_adj < 0.1f32.min(self.intensity * 0.5) && smoke > 1.0 {
            smoke += (self.intensity * 10.0).min(8.5) * (len_adj - 0.1);
        }
        if smoke > 1.0 {
            return [0; 4];
        }
        let mut colour = self.primary;
        if uy < 0.12 {
            let diff = 0.12 - uy;
            let modulation = (-2.0 + 0.5 * self.intensity * smoke) * diff;
            for (i, channel) in colour.iter_mut().enumerate() {
                *channel = self.primary[i] * (1.0 - 0.5 * diff) + 2.5 * diff * self.secondary[i];
                *channel = (*channel + *channel * modulation).max(0.0);
            }
        }
        [
            (colour[0] * 255.0).clamp(0.0, 255.0) as u8,
            (colour[1] * 255.0).clamp(0.0, 255.0) as u8,
            (colour[2] * 255.0).clamp(0.0, 255.0) as u8,
            255,
        ]
    }

    fn cell_density(&mut self, qx: f32, qy: f32) -> f32 {
        if self.cache && (-30.0..=30.0).contains(&qx) {
            let index = (qx as i32 + 30) as usize;
            let key = [qx.to_bits(), qy.to_bits()];
            #[cfg(all(target_arch = "arm", feature = "flame-simd"))]
            if self.simd
                && qx.to_bits() != (-0.0f32).to_bits()
                && (-30.0..=30.0).contains(&qy)
                && (self.cells[index].density.is_none() || self.cells[index].key != key)
            {
                self.fill_vector_cells(index, qy);
            }
            let cell = self.cells[index];
            if let Some(value) = cell.density.filter(|_| cell.key == key) {
                value
            } else {
                let value = self.density(qx / GRID, qy / GRID);
                self.cells[index] = Cell {
                    key,
                    density: Some(value),
                };
                value
            }
        } else {
            self.density(qx / GRID, qy / GRID)
        }
    }

    #[cfg(all(target_arch = "arm", feature = "flame-simd"))]
    fn fill_vector_cells(&mut self, index: usize, qy: f32) {
        extern "C" {
            fn balatro_flame_density4(params: *const f32, xs: *const f32, y: f32, output: *mut f32);
        }
        let first = index & !3;
        let columns = std::array::from_fn::<_, 4, _>(|n| (first + n) as f32 - 30.0);
        let xs = columns.map(|x| x / GRID);
        let params = [self.time, self.intensity, self.rise, self.scale, self.speed];
        let mut output = [0.0; 4];
        // The kernel reads five draw constants and four coordinates, and writes
        // four densities. Lua and image allocations are not shared with it.
        unsafe {
            balatro_flame_density4(params.as_ptr(), xs.as_ptr(), qy / GRID, output.as_mut_ptr());
        }
        for n in 0..4.min(self.cells.len() - first) {
            self.cells[first + n] = Cell {
                key: [columns[n].to_bits(), qy.to_bits()],
                density: Some(output[n]),
            };
        }
    }

    pub fn draw(&mut self, buffer: &mut crate::pixel_buffer::PixelBuffer, rect: [i32; 4]) {
        let [x, y, width, height] = rect;
        let x0 = x.max(0) as usize;
        let y0 = y.max(0) as usize;
        let x1 = ((x + width) as usize).min(buffer.width as usize);
        let y1 = ((y + height) as usize).min(buffer.height as usize);
        let w_range = x1.saturating_sub(x0).max(1);
        let h_range = y1.saturating_sub(y0).max(1);
        for py in y0..y1 {
            let uy = (py - y0) as f32 / h_range as f32 - 0.5;
            for px in x0..x1 {
                let ux = (px - x0) as f32 / w_range as f32 - 0.5;
                let [r, g, b, a] = self.pixel(ux, uy);
                if a > 0 {
                    let index = (py * buffer.width as usize + px) * 4;
                    if index + 3 < buffer.pixels.len() {
                        buffer.blend_at(index, r, g, b, a);
                    }
                }
            }
        }
    }

    fn density(&self, x: f32, y: f32) -> f32 {
        let wobble =
            0.01 * (-1.123 * x + 0.2 * self.time).sin() * (5.3332 * y + self.time * 0.931).cos();
        let usc_x = x + x * wobble;
        let usc_y = y + y * wobble;
        let mut sv_x = usc_x * self.scale;
        let mut sv_y = usc_y * self.scale + self.rise;
        let mut sv2_x = 0.0f32;
        let mut sv2_y = 0.0f32;
        for _ in 0..5 {
            let len = (sv_x * sv_x + sv_y * sv_y).sqrt();
            let noise = 0.3 * ((len * 0.411).cos() + 0.3344 * len.sin() - 0.23 * len.cos());
            let new_x = sv2_x + sv_x + 0.05 * sv2_y + noise;
            let new_y = sv2_y + sv_y + 0.05 * sv2_x + noise;
            sv2_x = new_x;
            sv2_y = new_y;
            sv_x += 0.5
                * (sv2_y.cos() + self.speed * 0.0812).cos()
                * (3.22 + sv2_x - self.speed * 0.1531).sin();
            sv_y += 0.5
                * (-sv2_x * 1.21222 + 0.113785 * self.speed).sin()
                * (sv2_y * 0.91213 - 0.13582 * self.speed).cos();
        }
        let dist_x = sv_x / self.scale * 5.0;
        let dist_y = (sv_y - self.rise) / self.scale * 5.0;
        let len_dist = (dist_x * dist_x + dist_y * dist_y).sqrt();
        let len_usc = (usc_x * usc_x + usc_y * usc_y).sqrt();
        let smoke =
            (len_dist + 0.1 * (len_usc - 0.5)).max(0.0) * (2.0 / (2.0 + self.intensity * 0.2));
        let fade =
            (2.0 - 0.3 * self.intensity).max(0.0) * (2.0 * (usc_y - 0.5) * (usc_y - 0.5)).max(0.0);
        smoke + fade
    }
}

#[cfg(test)]
mod tests;
