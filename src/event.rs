// Event ring buffer for uncalibrated-sextant.
//
// Records keypresses, rendered lines, and scene transitions as they
// occur during a run. Drained to the UEFI Serial protocol just before
// ACPI shutdown; see `Scene::drain_to_serial`.
//
// The wire-format types — `Event`, `Phase`, and `BootloaderChoice` —
// live in the shared `shakenfist-visual-digest` crate so that Sextant
// (the encoder) and Ryll (the future decoder) share a single source of
// truth. They are re-exported here so existing `crate::event::*`
// imports keep working unchanged. `RingBuffer<N>` stays Sextant-local:
// the encoder API takes a `&[&Event]` slice that the caller
// materialises from its container of choice, so the ring is purely an
// in-process buffer with no on-wire footprint.
//
// No std: fixed-size array with head/len indices, no VecDeque.

// Under the test profile, only `event.rs` (this file) and its
// dependencies compile; the renderer/scene/bootloader consumers that
// use `BootloaderChoice` and `Phase` are gated out, so the re-exports
// look unused. Production builds use all three; silence the warning
// for the test profile only.
#[cfg_attr(test, allow(unused_imports))]
pub use shakenfist_visual_digest::{BootloaderChoice, Event, Phase};

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
