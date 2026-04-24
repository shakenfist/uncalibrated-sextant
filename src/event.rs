// Event ring buffer for uncalibrated-sextant.
//
// Records keypresses, rendered lines, and scene transitions as they
// occur during a run. Phase 6 will drain this buffer to a serial
// log; for now it exists and is populated so Phase 6 has something
// to work with.
//
// No std: fixed-size array with head/len indices, no VecDeque.

/// Scene phases, ordered by progression through a single boot run.
#[derive(Copy, Clone, Debug)]
pub enum Phase {
    /// AWAITING OPERATOR — waiting for first keypress.
    Awaiting,
    /// Boot sequence playing out.
    Booting,
    /// Parked on SYSTEM ONLINE screen, waiting for final keypress.
    Parked,
}

/// Events recorded into the ring buffer during a run.
///
/// All variant fields are populated by `scene` and will be read by the
/// Phase 6 serial-drain code. They are intentionally dead from the
/// compiler's perspective until then.
#[allow(dead_code)] // Phase 6 will read these fields via serial drain.
#[derive(Copy, Clone, Debug)]
pub enum Event {
    /// A key was pressed by the operator.
    Keypress {
        unicode: char,
        scancode: u16,
        timestamp_ms: u64,
    },
    /// A text line was rendered to the screen.
    LineRendered { row: usize, timestamp_ms: u64 },
    /// The scene transitioned between phases.
    SceneTransition {
        from: Phase,
        to: Phase,
        timestamp_ms: u64,
    },
}

/// Fixed-capacity ring buffer, overwriting oldest entry on overflow.
///
/// `N` must be a power of two for efficient wrapping, though the
/// implementation does not enforce this. Capacity 256 is the default
/// instantiation in `Scene`.
pub struct RingBuffer<const N: usize> {
    entries: [Option<Event>; N],
    head: usize,
    len: usize,
}

impl<const N: usize> RingBuffer<N> {
    /// Create an empty ring buffer. `const fn` so it can initialise
    /// a `static` or `const` if needed.
    pub const fn new() -> Self {
        Self {
            // Option<Event> is Copy, so this is safe in a const context
            // with the repetition syntax.
            entries: [None; N],
            head: 0,
            len: 0,
        }
    }

    /// Push an event, overwriting the oldest entry when full.
    pub fn push(&mut self, event: Event) {
        if N == 0 {
            return;
        }
        if self.len < N {
            // Buffer not yet full: write at head + len.
            let idx = (self.head + self.len) % N;
            self.entries[idx] = Some(event);
            self.len += 1;
        } else {
            // Buffer full: overwrite head (oldest) and advance head.
            self.entries[self.head] = Some(event);
            self.head = (self.head + 1) % N;
        }
    }

    /// Number of events currently stored.
    // Phase 6 will call this to know how many events to drain.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.len
    }

    /// True if no events have been stored yet.
    // Provided as the conventional companion to `len`; Phase 6 will use it.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}
