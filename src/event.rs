// Event ring buffer for uncalibrated-sextant.
//
// Records keypresses, rendered lines, and scene transitions as they
// occur during a run. Drained to the UEFI Serial protocol just before
// ACPI shutdown; see `Scene::drain_to_serial`.
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

impl Phase {
    /// Short lowercase tag used by the serial drain's one-line-per-event
    /// format. Kept stable so Ryll's future parser can match literally.
    pub fn tag(&self) -> &'static str {
        match self {
            Phase::Awaiting => "awaiting",
            Phase::Booting => "booting",
            Phase::Parked => "parked",
        }
    }
}

/// Operator's choice at the locked-bootloader R/I/A prompt.
#[derive(Copy, Clone, Debug)]
pub enum BootloaderChoice {
    /// Operator chose (R)etry — re-run the decryption attempt.
    Retry,
    /// Operator chose (I)gnore — proceed to the paste blob screen.
    Ignore,
    /// Operator chose (A)bort — cold-reset immediately.
    Abort,
}

impl BootloaderChoice {
    /// Short lowercase tag used by the serial drain's one-line-per-event
    /// format. Kept stable so Ryll's future parser can match literally.
    pub fn tag(&self) -> &'static str {
        match self {
            BootloaderChoice::Retry => "retry",
            BootloaderChoice::Ignore => "ignore",
            BootloaderChoice::Abort => "abort",
        }
    }
}

/// Events recorded into the ring buffer during a run.
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
    /// Operator made a choice at the locked-bootloader R/I/A prompt.
    BootloaderDecision {
        choice: BootloaderChoice,
        /// 1-indexed count of times the prompt has been rendered so far.
        attempt: u32,
        timestamp_ms: u64,
    },
    /// A paste was received and validated at the awaiting-payload prompt.
    PasteReceived {
        /// Number of bytes in the paste (excluding any trailing CR/LF terminator).
        len: usize,
        /// Whether the paste matched the expected payload byte-exactly.
        correct: bool,
        timestamp_ms: u64,
    },
    /// The silent-wait timer elapsed; the visible countdown is about to begin.
    BootloaderTimeout { timestamp_ms: u64 },
    /// GOP mode switched (or attempted to switch) at the
    /// operator's request.
    ModeSwitch {
        requested_w: u32,
        requested_h: u32,
        applied_w: u32,
        applied_h: u32,
        timestamp_ms: u64,
    },
    /// Cycle-through-all-modes walk completed (or was
    /// interrupted). `count` is the number of mode switches
    /// performed during the cycle.
    // Emitter wired in Phase 2; suppress dead-code lint until then.
    #[allow(dead_code)]
    ModeCycle {
        count: u32,
        interrupted: bool,
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

    /// Iterate over events in chronological order (oldest first).
    pub fn iter(&self) -> impl Iterator<Item = &Event> {
        (0..self.len).filter_map(move |i| {
            let idx = (self.head + i) % N;
            self.entries[idx].as_ref()
        })
    }
}
