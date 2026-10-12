//! A capacity-bounded stack for the frames of open nodes.

/// A stack that never holds more than its capacity. Storage grows on
/// demand up to the capacity, so shallow documents stay small and deep
/// ones fail closed instead of growing without bound.
pub(crate) struct FrameStack<T> {
    frames: Vec<T>,
    capacity: usize,
}

impl<T> FrameStack<T> {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            frames: Vec::new(),
            capacity,
        }
    }

    pub(crate) fn is_full(&self) -> bool {
        self.frames.len() >= self.capacity
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.frames.len()
    }

    /// Push a frame; a full stack hands the frame back.
    pub(crate) fn push(&mut self, frame: T) -> Result<(), T> {
        if self.is_full() {
            return Err(frame);
        }
        self.frames.push(frame);
        Ok(())
    }

    pub(crate) fn pop(&mut self) -> Option<T> {
        self.frames.pop()
    }

    pub(crate) fn last_mut(&mut self) -> Option<&mut T> {
        self.frames.last_mut()
    }
}
