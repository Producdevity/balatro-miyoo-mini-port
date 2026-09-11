use super::*;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn a_full_display_queue_waits_instead_of_replacing_the_frame() {
    let queue = Arc::new(PresenterQueue {
        pending: Mutex::new(Some(FrameSubmission {
            width: 1, height: 1, pixels: vec![1, 2, 3, 255],
        })),
        recycled: Mutex::new(Vec::new()),
        ready: Condvar::new(),
        error: Mutex::new(None),
        submitted: AtomicU64::new(1),
        displayed: AtomicU64::new(0),
    });
    let producer = Arc::clone(&queue);
    let (started, ready) = mpsc::channel();
    let (done, result) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        let presenter = AsyncPresenter { queue: producer };
        let mut frame = PixelBuffer::new(1, 1);
        frame.pixels.copy_from_slice(&[4, 5, 6, 255]);
        started.send(()).unwrap();
        done.send(presenter.submit(&frame)).unwrap();
    });
    ready.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(matches!(result.recv_timeout(Duration::from_millis(20)), Err(mpsc::RecvTimeoutError::Timeout)));
    assert_eq!(queue.pending.lock().unwrap().take().unwrap().pixels, [1, 2, 3, 255]);
    queue.displayed.fetch_add(1, Ordering::Relaxed);
    queue.ready.notify_all();
    result.recv_timeout(Duration::from_secs(2)).unwrap().unwrap();
    thread.join().unwrap();
    assert_eq!(queue.pending.lock().unwrap().take().unwrap().pixels, [4, 5, 6, 255]);
    assert_eq!(queue.submitted.load(Ordering::Relaxed), 2);
    queue.displayed.fetch_add(1, Ordering::Relaxed);
    let presenter = AsyncPresenter { queue };
    presenter.finish().unwrap();
    *presenter.queue.error.lock().unwrap() = Some("display disconnected".into());
    assert!(presenter.submit(&PixelBuffer::new(1, 1)).unwrap_err().to_string().contains("display disconnected"));
}
