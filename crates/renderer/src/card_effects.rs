#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CardShaderInputs {
    pub phase: f32,
    pub clock: f32,
    pub seed: f32,
}

const TRIG_LUT_SIZE: usize = 4096;
const TRIG_LUT_SCALE: f32 = TRIG_LUT_SIZE as f32 / std::f32::consts::TAU;

static SIN_LUT_GLOBAL: std::sync::LazyLock<[f32; TRIG_LUT_SIZE]> = std::sync::LazyLock::new(|| {
    let mut lut = [0.0f32; TRIG_LUT_SIZE];
    for i in 0..TRIG_LUT_SIZE {
        lut[i] = (i as f32 * std::f32::consts::TAU / TRIG_LUT_SIZE as f32).sin();
    }
    lut
});

#[inline(always)]
pub(crate) fn fast_sin(x: f32) -> f32 {
    sample_sin(&SIN_LUT_GLOBAL, x)
}

#[inline(always)]
fn sample_sin(lut: &[f32; TRIG_LUT_SIZE], x: f32) -> f32 {
    let phase = x * TRIG_LUT_SCALE;
    let rounded = (phase + if phase >= 0.0 { 0.5 } else { -0.5 }) as i32;
    let idx = (rounded as usize) & (TRIG_LUT_SIZE - 1);
    lut[idx]
}

#[derive(Clone, Copy)]
pub(crate) struct TrigLookup(&'static [f32; TRIG_LUT_SIZE]);

impl TrigLookup {
    pub(crate) fn new() -> Self {
        Self(&SIN_LUT_GLOBAL)
    }

    #[inline(always)]
    pub(crate) fn sin(self, x: f32) -> f32 {
        sample_sin(self.0, x)
    }

    #[inline(always)]
    pub(crate) fn cos(self, x: f32) -> f32 {
        self.sin(x + std::f32::consts::FRAC_PI_2)
    }
}

#[inline(always)]
pub(crate) fn fast_cos(x: f32) -> f32 {
    fast_sin(x + std::f32::consts::FRAC_PI_2)
}

/// Values shared by every pixel of one card draw.
#[derive(Clone, Copy)]
pub(crate) struct ShaderPre {
    trig: TrigLookup,
    pub(crate) texture_size: [f32; 2],
    pub(crate) negative_invert: bool,
    // Foil (effect 3)
    pub(crate) foil_r: f32,
    pub(crate) foil_g: f32,
    pub(crate) foil_rot_x: f32,
    pub(crate) foil_rot_y: f32,
    pub(crate) foil_rot_len: f32,
    pub(crate) foil_inner_sin: f32, // fast_sin(foil_r * 1.65 + 0.2 * foil_g)
    pub(crate) foil_cos7: f32,      // fast_cos(foil_r * 7.0)
    pub(crate) foil_cos3414: f32,   // fast_cos(foil_r * 3.414)
    // Holo (effect 4) , time-only noise field offsets
    pub(crate) holo_x: f32,
    pub(crate) holo_t: f32,
    pub(crate) holo_off: [f32; 6], // sin1, cos1, cos2, cos2y, sin3, sin3y
    // Poly (effect 5)
    pub(crate) poly_x: f32,
    pub(crate) poly_y: f32,
    pub(crate) poly_t: f32,
    pub(crate) poly_off: [f32; 6],
    // Hologram (effect 9)
    pub(crate) holo9_sin_g: f32,
    pub(crate) holo9_sin_r: f32,
    // Voucher/Booster/Neg_shine (effects 7/8/10) , time/28 precomputed
    pub(crate) t28: f32,
    // Gold seal (effect 11)
    pub(crate) gs_t: f32,
    pub(crate) gs_sin6t: f32,
}

impl ShaderPre {
    pub(crate) fn sine_table(&self) -> *const f32 {
        self.trig.0.as_ptr()
    }

    #[inline(always)]
    pub(crate) fn sin(&self, x: f32) -> f32 {
        self.trig.sin(x)
    }

    #[inline(always)]
    pub(crate) fn cos(&self, x: f32) -> f32 {
        self.sin(x + std::f32::consts::FRAC_PI_2)
    }

    pub(crate) fn compute(effect: u8, inputs: CardShaderInputs) -> Self {
        let mut sp = Self {
            // Resolve LazyLock once per draw, outside every per-pixel lookup.
            trig: TrigLookup::new(),
            texture_size: [71.0, 95.0],
            negative_invert: inputs.clock != 0.0,
            foil_r: 0.0,
            foil_g: 0.0,
            foil_rot_x: 0.0,
            foil_rot_y: 0.0,
            foil_rot_len: 1.0,
            foil_inner_sin: 0.0,
            foil_cos7: 0.0,
            foil_cos3414: 0.0,
            holo_x: 0.0,
            holo_t: 0.0,
            holo_off: [0.0; 6],
            poly_x: 0.0,
            poly_y: 0.0,
            poly_t: 0.0,
            poly_off: [0.0; 6],
            holo9_sin_g: 0.0,
            holo9_sin_r: 0.0,
            t28: 0.0,
            gs_t: 0.0,
            gs_sin6t: 0.0,
        };
        match effect {
            3 => {
                sp.foil_r = inputs.phase;
                sp.foil_g = inputs.clock;
                sp.foil_rot_x = fast_cos(sp.foil_r * 0.1221);
                sp.foil_rot_y = fast_sin(sp.foil_r * 0.3512);
                sp.foil_rot_len =
                    (sp.foil_rot_x * sp.foil_rot_x + sp.foil_rot_y * sp.foil_rot_y).sqrt();
                sp.foil_inner_sin = fast_sin(sp.foil_r * 1.65 + 0.2 * sp.foil_g);
                sp.foil_cos7 = fast_cos(sp.foil_r * 7.0);
                sp.foil_cos3414 = fast_cos(sp.foil_r * 3.414);
            }
            4 => {
                sp.holo_x = inputs.phase;
                sp.holo_t = inputs.clock * 7.221 + inputs.seed;
                let t = sp.holo_t;
                sp.holo_off = [
                    50.0 * fast_sin(-t / 143.634),
                    50.0 * fast_cos(-t / 99.4324),
                    50.0 * fast_cos(t / 53.1532),
                    50.0 * fast_cos(t / 61.4532),
                    50.0 * fast_sin(-t / 87.53218),
                    50.0 * fast_sin(-t / 49.0),
                ];
            }
            5 => {
                sp.poly_x = inputs.phase;
                sp.poly_y = inputs.clock;
                sp.poly_t = inputs.clock * 2.221 + inputs.seed;
                let t = sp.poly_t;
                sp.poly_off = [
                    50.0 * fast_sin(-t / 143.634),
                    50.0 * fast_cos(-t / 99.4324),
                    50.0 * fast_cos(t / 53.1532),
                    50.0 * fast_cos(t / 61.4532),
                    50.0 * fast_sin(-t / 87.53218),
                    50.0 * fast_sin(-t / 49.0),
                ];
            }
            7 | 8 | 10 => {
                sp.t28 = inputs.phase;
            }
            9 => {
                let holo_r = inputs.phase;
                let holo_g = inputs.clock;
                sp.holo9_sin_g = fast_sin(2.0 * holo_g);
                sp.holo9_sin_r = fast_sin(holo_r * 3.0);
            }
            11 => {
                sp.gs_t = inputs.phase;
                sp.gs_sin6t = fast_sin(sp.gs_t * 6.0);
            }
            _ => {}
        }
        sp
    }
}

// The second component holds holo's separate grid term.
#[inline(always)]
#[expect(
    clippy::approx_constant,
    reason = "Match the original shader's literal 3.14"
)]
pub(crate) fn spatial(effect: u8, sp: &ShaderPre, ux: f32, uy: f32) -> [f32; 2] {
    let fast_sin = |x| sp.sin(x);
    let fast_cos = |x| sp.cos(x);
    match effect {
        3 => {
            let foil_r = sp.foil_r;
            let foil_g = sp.foil_g;
            let ax = (ux - 0.5) * (sp.texture_size[0] / sp.texture_size[1]);
            let ay = uy - 0.5;
            let len_uv = (ax * ax + ay * ay).sqrt();
            let len90 = len_uv * 90.0;

            let fac =
                (2.0 * fast_sin(
                    len90
                        + foil_r * 2.0
                        + 3.0 * (1.0 + 0.8 * fast_cos(len_uv * 113.1121 - foil_r * 3.121)),
                ) - 1.0
                    - (5.0 - len90).max(0.0))
                .clamp(0.0, 1.0);

            // Angle-based component (rot_x, rot_y, rot_len precomputed)
            let uv_len = len_uv.max(0.001);
            let angle = (sp.foil_rot_x * ax + sp.foil_rot_y * ay) / (sp.foil_rot_len * uv_len);
            let fac2 = (5.0
                * fast_cos(foil_g * 0.3 + angle * 3.14 * (2.2 + 0.9 * sp.foil_inner_sin))
                - 4.0
                - (2.0 - len_uv * 20.0).max(0.0))
            .clamp(0.0, 1.0);

            let fac3 = 0.3
                * (2.0 * fast_sin(foil_r * 5.0 + ux * 3.0 + 3.0 * (1.0 + 0.5 * sp.foil_cos7))
                    - 1.0)
                    .clamp(-1.0, 1.0);
            let fac4 = 0.3
                * (2.0 * fast_sin(foil_r * 6.66 + uy * 3.8 + 3.0 * (1.0 + 0.5 * sp.foil_cos3414))
                    - 1.0)
                    .clamp(-1.0, 1.0);

            let maxfac = (fac.max(fac2.max(fac3.max(fac4.max(0.0))))
                + 2.2 * (fac + fac2 + fac3 + fac4))
                .max(0.0);

            [maxfac, 0.0]
        }
        4 => {
            // Noise field with precomputed time-only offsets
            let fuv_x = (floored_uv(ux, sp.texture_size[0]) - 0.5) * 250.0;
            let fuv_y = (floored_uv(uy, sp.texture_size[1]) - 0.5) * 250.0;
            let f1x = fuv_x + sp.holo_off[0];
            let f1y = fuv_y + sp.holo_off[1];
            let f2x = fuv_x + sp.holo_off[2];
            let f2y = fuv_y + sp.holo_off[3];
            let f3x = fuv_x + sp.holo_off[4];
            let f3y = fuv_y + sp.holo_off[5];
            let field_len1 = (f1x * f1x + f1y * f1y).sqrt();
            let field_len2 = (f2x * f2x + f2y * f2y).sqrt();
            let field_len3 = (f3x * f3x + f3y * f3y).sqrt();
            let field = (1.0
                + fast_cos(field_len1 / 19.483)
                + fast_sin(field_len2 / 33.155) * fast_cos(f2y / 15.73)
                + fast_cos(field_len3 / 27.193) * fast_sin(f3x / 21.92))
                / 2.0;
            let res = 0.5 + 0.5 * fast_cos(sp.holo_x * 2.612 + (field - 0.5) * 3.14);

            // Grid pattern (hexagonal-ish)
            let gridsize = 0.79_f32;
            let grid1 = (7.0 * fast_cos(ux * gridsize * 20.0).abs() - 6.0).max(0.0);
            let grid2 =
                (7.0 * fast_cos(uy * gridsize * 45.0 + ux * gridsize * 20.0) - 6.0).max(0.0);
            let grid3 =
                (7.0 * fast_cos(uy * gridsize * 45.0 - ux * gridsize * 20.0) - 6.0).max(0.0);
            let fac = 0.5 * grid1.max(grid2.max(grid3));

            [res, fac]
        }
        5 => {
            // 3-part noise field , time-only offsets precomputed in sp.poly_off
            let fuv_x = (floored_uv(ux, sp.texture_size[0]) - 0.5) * 50.0;
            let fuv_y = (floored_uv(uy, sp.texture_size[1]) - 0.5) * 50.0;
            let f1x = fuv_x + sp.poly_off[0];
            let f1y = fuv_y + sp.poly_off[1];
            let f2x = fuv_x + sp.poly_off[2];
            let f2y = fuv_y + sp.poly_off[3];
            let f3x = fuv_x + sp.poly_off[4];
            let f3y = fuv_y + sp.poly_off[5];
            let field_len1 = (f1x * f1x + f1y * f1y).sqrt();
            let field_len2 = (f2x * f2x + f2y * f2y).sqrt();
            let field_len3 = (f3x * f3x + f3y * f3y).sqrt();
            let field = (1.0
                + fast_cos(field_len1 / 19.483)
                + fast_sin(field_len2 / 33.155) * fast_cos(f2y / 15.73)
                + fast_cos(field_len3 / 27.193) * fast_sin(f3x / 21.92))
                / 2.0;
            let res = 0.5 + 0.5 * fast_cos(sp.poly_x * 2.612 + (field - 0.5) * 3.14);

            [res, 0.0]
        }
        _ => [0.0; 2],
    }
}

#[inline(always)]
fn floored_uv(uv: f32, size: f32) -> f32 {
    // Raster paths reject coordinates outside the source rectangle.
    (uv * size) as u32 as f32 / size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_local_trig_preserves_lookup_bits() {
        let pre = ShaderPre::compute(0, CardShaderInputs::default());
        for step in -100_000..=100_000 {
            let phase = step as f32 / 31.0;
            assert_eq!(pre.sin(phase).to_bits(), fast_sin(phase).to_bits());
            assert_eq!(pre.cos(phase).to_bits(), fast_cos(phase).to_bits());
        }
        for phase in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX] {
            assert_eq!(pre.sin(phase).to_bits(), fast_sin(phase).to_bits());
        }
    }

    #[test]
    fn texel_coordinates_match_floor_in_the_raster_domain() {
        for size in [1.0, 71.0, 95.0, 142.0, 190.0] {
            for step in 0..10_000 {
                let uv = step as f32 / 10_000.0;
                assert_eq!(floored_uv(uv, size), (uv * size).floor() / size);
            }
        }
    }

    #[test]
    fn shader_precomputation_uses_the_game_phase_clock_and_seed() {
        let inputs = CardShaderInputs {
            phase: 2.25,
            clock: 44.0,
            seed: 987.125,
        };
        let foil = ShaderPre::compute(3, inputs);
        assert_eq!((foil.foil_r, foil.foil_g), (inputs.phase, inputs.clock));
        let holo = ShaderPre::compute(4, inputs);
        assert_eq!(
            (holo.holo_x, holo.holo_t),
            (inputs.phase, inputs.clock * 7.221 + inputs.seed)
        );
        let poly = ShaderPre::compute(5, inputs);
        assert_eq!(
            (poly.poly_x, poly.poly_y, poly.poly_t),
            (
                inputs.phase,
                inputs.clock,
                inputs.clock * 2.221 + inputs.seed
            )
        );
        assert!(!ShaderPre::compute(6, CardShaderInputs::default()).negative_invert);
        assert!(ShaderPre::compute(6, inputs).negative_invert);
    }

    #[test]
    #[expect(
        clippy::approx_constant,
        reason = "Reference uses the shader's literal 3.14"
    )]
    fn noise_field_tracks_full_precision_trigonometry() {
        let mut maximum = 0.0_f32;
        for effect in [4, 5] {
            for clock in [0.0, 31.25, 1234.0, 9123.5] {
                for seed in [0.0, 178.125, 2999.0] {
                    let inputs = CardShaderInputs {
                        phase: clock / 28.0 + 0.125,
                        clock,
                        seed,
                    };
                    let sp = ShaderPre::compute(effect, inputs);
                    let t = clock * if effect == 4 { 7.221 } else { 2.221 } + seed;
                    for y in 0..48 {
                        for x in 0..36 {
                            let ux = x as f32 / 36.0;
                            let uy = y as f32 / 48.0;
                            let scale = if effect == 4 { 250.0 } else { 50.0 };
                            let u = ((ux * 71.0).floor() / 71.0 - 0.5) * scale;
                            let v = ((uy * 95.0).floor() / 95.0 - 0.5) * scale;
                            let a = [
                                u + 50.0 * (-t / 143.634).sin(),
                                v + 50.0 * (-t / 99.4324).cos(),
                            ];
                            let b = [
                                u + 50.0 * (t / 53.1532).cos(),
                                v + 50.0 * (t / 61.4532).cos(),
                            ];
                            let c = [
                                u + 50.0 * (-t / 87.53218).sin(),
                                v + 50.0 * (-t / 49.0).sin(),
                            ];
                            let length = |p: [f32; 2]| (p[0] * p[0] + p[1] * p[1]).sqrt();
                            let field = (1.0
                                + (length(a) / 19.483).cos()
                                + (length(b) / 33.155).sin() * (b[1] / 15.73).cos()
                                + (length(c) / 27.193).cos() * (c[0] / 21.92).sin())
                                / 2.0;
                            let expected =
                                0.5 + 0.5 * (inputs.phase * 2.612 + (field - 0.5) * 3.14).cos();
                            maximum =
                                maximum.max((spatial(effect, &sp, ux, uy)[0] - expected).abs());
                        }
                    }
                }
            }
        }
        assert!(maximum < 0.01, "maximum field error={maximum}");
    }
}
