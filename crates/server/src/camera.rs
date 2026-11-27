//! An emulator's camera is a V4L2 device that the shim inside it (crates/vcam)
//! makes up. Its frames are YUV 4:2:0 in three planes at limited range, and
//! only the newest is kept.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::watch;

pub(crate) struct Fed {
    width: usize,
    height: usize,
    frames: watch::Sender<Arc<Vec<u8>>>,
    /// The page crops what it sends by this.
    upright: AtomicBool,
}

impl Fed {
    pub(crate) fn start(width: usize, height: usize) -> Self {
        let fed = Self {
            width,
            height,
            frames: watch::Sender::new(Arc::new(Vec::new())),
            upright: AtomicBool::new(true),
        };

        fed.black();

        fed
    }

    pub(crate) fn frames(&self) -> watch::Receiver<Arc<Vec<u8>>> {
        self.frames.subscribe()
    }

    pub(crate) fn upright(&self) -> bool {
        self.upright.load(Ordering::Relaxed)
    }

    pub(crate) fn turn(&self, upright: bool) {
        self.upright.store(upright, Ordering::Relaxed);
    }

    pub(crate) fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub(crate) fn raw(&self, frame: Vec<u8>) {
        if frame.len() == self.width * self.height * 3 / 2 {
            self.frames.send_replace(Arc::new(frame));
        }
    }

    pub(crate) fn black(&self) {
        let mut black = vec![128; self.width * self.height * 3 / 2];

        // Black is a luma of 16 in limited range, and all zeros would be green.
        black[..self.width * self.height].fill(16);

        self.frames.send_replace(Arc::new(black));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_newest_frame_of_its_size() {
        let fed = Fed::start(4, 2);
        let mut frames = fed.frames();

        assert_eq!(
            **frames.borrow(),
            [16, 16, 16, 16, 16, 16, 16, 16, 128, 128, 128, 128]
        );

        fed.raw(vec![7; 11]);
        assert!(!frames.has_changed().unwrap());

        fed.raw(vec![7; 12]);
        assert_eq!(**frames.borrow_and_update(), [7; 12]);
    }
}
