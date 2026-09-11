use crate::occlusion::Rect;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Layer {
    pub image: usize,
    pub region: [u32; 4],
    pub transform: [u32; 6],
    pub effect: u8,
    pub inputs: [u32; 5],
    pub clip: Rect,
    pub blend: u8,
}

impl Layer {
    pub(crate) fn follows(self, earlier: Self) -> bool {
        self.transform == earlier.transform
            && self.region[2..] == earlier.region[2..]
            && self.effect == earlier.effect
            && self.inputs == earlier.inputs
            && self.clip == earlier.clip
            && self.blend == earlier.blend
            && (self.image != earlier.image || self.region != earlier.region)
    }
}

#[cfg(any(test, feature = "native-sampler"))]
#[derive(Default)]
pub(crate) struct Probe {
    previous: Option<Layer>,
    eligible: u64,
    pairs: u64,
    pair_bounds: u64,
}

#[cfg(any(test, feature = "native-sampler"))]
impl Probe {
    pub fn observe(&mut self, layer: Option<Layer>, bounds: Option<Rect>) {
        self.eligible += u64::from(layer.is_some());
        if let (Some(earlier), Some(current)) = (self.previous, layer) {
            if current.follows(earlier) {
                self.pairs += 1;
                self.pair_bounds += bounds.map_or(0, |rect| rect.area() as u64);
                self.previous = None;
                return;
            }
        }
        self.previous = layer;
    }

    pub fn boundary(&mut self) {
        self.previous = None;
    }
}

#[cfg(any(test, feature = "native-sampler"))]
impl Drop for Probe {
    fn drop(&mut self) {
        eprintln!(
            "[layer-probe] eligible={} adjacent_pairs={} pair_bounds={}",
            self.eligible, self.pairs, self.pair_bounds
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(image: usize) -> Layer {
        Layer {
            image,
            region: [0, 0, 71.0_f32.to_bits(), 95.0_f32.to_bits()],
            transform: [1; 6],
            effect: 4,
            inputs: [0; 5],
            clip: Rect([0, 0, 640, 480]),
            blend: 0,
        }
    }

    #[test]
    fn pairing_requires_matching_effect_geometry_and_state() {
        let first = layer(1);
        let second = layer(2);
        assert!(second.follows(first));
        assert!(!first.follows(first));
        let mut changed = second;
        changed.transform[3] += 1;
        assert!(!changed.follows(first));
        changed = second;
        changed.inputs[1] += 1;
        assert!(!changed.follows(first));
        changed = second;
        changed.region[2] += 1;
        assert!(!changed.follows(first));
        changed = second;
        changed.effect = 5;
        assert!(!changed.follows(first));
        changed = second;
        changed.clip = Rect([1, 0, 640, 480]);
        assert!(!changed.follows(first));
        changed = second;
        changed.blend = 4;
        assert!(!changed.follows(first));
    }

    #[test]
    fn pairs_do_not_overlap_or_cross_unrelated_draws_and_boundaries() {
        let mut probe = Probe::default();
        let bounds = Some(Rect([0, 0, 71, 95]));
        for image in 1..=4 {
            probe.observe(Some(layer(image)), bounds);
        }
        assert_eq!(probe.pairs, 2);
        probe.observe(Some(layer(1)), bounds);
        probe.observe(None, bounds);
        probe.observe(Some(layer(2)), bounds);
        probe.boundary();
        probe.observe(Some(layer(3)), bounds);
        assert_eq!(probe.pairs, 2);
        assert_eq!(probe.pair_bounds, 71 * 95 * 2);
    }
}
