use std::{marker::PhantomData, rc::Rc};

unsafe extern "C" {
    fn balatro_copy_trace_begin() -> i32;
    fn balatro_copy_trace_end();
}

// Linker wrapping records external copy calls, not copies inlined by the compiler.
// Keep the scope on its creating thread; the C wrappers use thread-local enablement.
pub struct CopyTrace(PhantomData<Rc<()>>);

impl CopyTrace {
    pub fn start() -> Option<Self> {
        (unsafe { balatro_copy_trace_begin() } != 0).then_some(Self(PhantomData))
    }
}

impl Drop for CopyTrace {
    fn drop(&mut self) {
        unsafe { balatro_copy_trace_end() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" {
        fn __wrap_memcpy(dest: *mut u8, src: *const u8, count: usize) -> *mut u8;
        fn __wrap_memmove(dest: *mut u8, src: *const u8, count: usize) -> *mut u8;
    }

    #[test]
    fn wrappers_preserve_copies_overlaps_and_scope_ownership() {
        for _ in 0..2 {
            let scope = CopyTrace::start().expect("one trace owner");
            assert!(CopyTrace::start().is_none());
            let source: Vec<u8> = (0..256).map(|value| value as u8).collect();
            let mut actual = [0; 256];
            unsafe {
                assert_eq!(
                    __wrap_memcpy(actual.as_mut_ptr(), source.as_ptr(), source.len()),
                    actual.as_mut_ptr()
                );
            }
            let mut expected = actual;
            for (from, to, count) in [(0, 13, 141), (17, 0, 139), (0, 0, 256), (0, 5, 0)] {
                expected.copy_within(from..from + count, to);
                unsafe {
                    __wrap_memmove(
                        actual.as_mut_ptr().add(to),
                        actual.as_ptr().add(from),
                        count,
                    );
                }
                assert_eq!(actual, expected);
            }
            drop(scope);
        }
    }
}
