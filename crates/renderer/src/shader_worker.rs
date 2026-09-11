use super::{raster, ColourCache, Uniforms};
use crate::neon_sprite::Sprite;
use std::sync::{mpsc, LazyLock, Mutex};

static MIN_PIXELS: LazyLock<i64> = LazyLock::new(|| {
    if cfg!(test) {
        return 0;
    }
    std::env::var("BALATRO_SHADER_WORKER_MIN_PIXELS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| (256..=65536).contains(value))
        .unwrap_or(4096)
});

struct Draw {
    effect: u8,
    uniforms: *const Uniforms,
    cache: bool,
    sprite: Sprite,
    source: *const u8,
    target: *mut u8,
    #[cfg(feature = "layer-pairs")]
    overlay: Option<(Sprite, *const u8)>,
}

// Draw never escapes draw(): its Pending guard waits before any borrowed
// allocation can be released. Only disjoint, validated destination rows are
// written concurrently. Source pixels and uniforms are read-only.
unsafe impl Send for Draw {}

impl Draw {
    unsafe fn run(&self, cache: Option<&mut ColourCache>) {
        #[cfg(feature = "layer-pairs")]
        if let Some((overlay, source)) = &self.overlay {
            super::raster_pair(
                self.effect,
                &self.sprite,
                overlay,
                &*self.uniforms,
                self.source,
                *source,
                self.target,
                if self.cache { cache } else { None },
            );
            return;
        }
        raster(
            self.effect,
            &self.sprite,
            &*self.uniforms,
            self.source,
            self.target,
            if self.cache { cache } else { None },
        );
    }
}

struct Worker {
    send: Option<mpsc::Sender<Draw>>,
    done: mpsc::Receiver<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Worker {
    fn new() -> std::io::Result<Self> {
        let (send, jobs) = mpsc::channel::<Draw>();
        let (finished, done) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("card-shader".into())
            .stack_size(256 * 1024)
            .spawn(move || {
                let mut cache = ColourCache::new();
                while let Ok(draw) = jobs.recv() {
                    unsafe {
                        draw.run(Some(&mut cache));
                    }
                    if finished.send(()).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            send: Some(send),
            done,
            thread: Some(thread),
        })
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.send.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Pending<'a>(&'a Worker);

impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.0
            .done
            .recv()
            .expect("card shader stopped before completing its draw");
    }
}

pub(super) fn draw(
    effect: u8,
    sprite: &Sprite,
    uniforms: &Uniforms,
    source: &[u8],
    target: &mut [u8],
    cache: Option<&mut ColourCache>,
) -> bool {
    dispatch(
        effect,
        sprite,
        uniforms,
        source,
        target,
        cache,
        #[cfg(feature = "layer-pairs")]
        None,
    )
}

#[cfg(feature = "layer-pairs")]
pub(super) fn draw_pair(
    effect: u8,
    sprite: &Sprite,
    overlay: &Sprite,
    uniforms: &Uniforms,
    source: &[u8],
    overlay_source: &[u8],
    target: &mut [u8],
    cache: Option<&mut ColourCache>,
) -> bool {
    dispatch(
        effect,
        sprite,
        uniforms,
        source,
        target,
        cache,
        Some((overlay, overlay_source)),
    )
}

fn dispatch(
    effect: u8,
    sprite: &Sprite,
    uniforms: &Uniforms,
    source: &[u8],
    target: &mut [u8],
    cache: Option<&mut ColourCache>,
    #[cfg(feature = "layer-pairs")] overlay: Option<(&Sprite, &[u8])>,
) -> bool {
    static ENABLED: LazyLock<bool> = LazyLock::new(|| {
        cfg!(test) || std::env::var("BALATRO_SHADER_WORKER").as_deref() == Ok("1")
    });
    if !*ENABLED || !matches!(effect, 3..=5) {
        return false;
    }
    let Some((first, second)) = sprite.split_rows(*MIN_PIXELS) else {
        return false;
    };
    #[cfg(feature = "layer-pairs")]
    let (overlay_first, overlay_second) = if let Some((sprite, source)) = overlay {
        let Some((first, second)) = sprite.split_rows(*MIN_PIXELS) else {
            return false;
        };
        (
            Some((first, source.as_ptr())),
            Some((second, source.as_ptr())),
        )
    } else {
        (None, None)
    };
    static WORKER: LazyLock<Option<Mutex<Worker>>> = LazyLock::new(|| {
        Worker::new()
            .map(|worker| {
                eprintln!(
                    "[shader-worker] row splitting enabled; minimum_pixels={}",
                    *MIN_PIXELS
                );
                Mutex::new(worker)
            })
            .map_err(|error| {
                eprintln!("[shader-worker] unavailable: {error}");
            })
            .ok()
    });
    let Some(worker) = WORKER.as_ref() else {
        return false;
    };
    let worker = worker.lock().expect("card shader queue poisoned");
    let pixels = target.as_mut_ptr();
    let job = Draw {
        effect,
        uniforms,
        cache: cache.is_some(),
        sprite: second,
        source: source.as_ptr(),
        target: pixels,
        #[cfg(feature = "layer-pairs")]
        overlay: overlay_second,
    };
    worker
        .send
        .as_ref()
        .unwrap()
        .send(job)
        .expect("card shader worker stopped");
    let _pending = Pending(&worker);
    unsafe {
        Draw {
            effect,
            uniforms,
            cache: cache.is_some(),
            sprite: first,
            source: source.as_ptr(),
            target: pixels,
            #[cfg(feature = "layer-pairs")]
            overlay: overlay_first,
        }
        .run(cache);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_effects::{CardShaderInputs, ShaderPre};
    use crate::pixel_buffer::PixelBuffer;
    use crate::shader_batch::ShaderBatch;

    #[test]
    fn pending_draw_finishes_before_unwinding_releases_its_buffers() {
        let source = vec![127; 17 * 13 * 4];
        let mut actual = PixelBuffer::new(43, 31);
        let mut expected = PixelBuffer::new(43, 31);
        let sprite = Sprite::new(
            &actual,
            &source,
            17,
            [0.0, 0.0, 17.0, 13.0],
            [0, 0, 43, 31],
            [0.4, 0.0, 0.0, 0.0, 0.4, 0.0],
            [255; 4],
            false,
            false,
        )
        .unwrap();
        let batch = ShaderBatch::create(4, &ShaderPre::compute(4, CardShaderInputs::default()));
        unsafe {
            raster(
                4,
                &sprite,
                &batch.uniforms,
                source.as_ptr(),
                expected.pixels.as_mut_ptr(),
                None,
            );
        }
        let worker = Worker::new().unwrap();
        let job = Draw {
            effect: 4,
            uniforms: &batch.uniforms,
            cache: true,
            sprite,
            source: source.as_ptr(),
            target: actual.pixels.as_mut_ptr(),
            #[cfg(feature = "layer-pairs")]
            overlay: None,
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            worker.send.as_ref().unwrap().send(job).unwrap();
            let _pending = Pending(&worker);
            panic!("interrupt the calling draw");
        }));
        assert!(result.is_err());
        assert_eq!(actual.pixels, expected.pixels);
        drop(worker);
    }
}
