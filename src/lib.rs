//! Integer-only, allocation-free anomaly detector for sensor streams.
//!
//! - `no_std`, no heap, no floating point, no `unsafe`. Checked: builds for
//!   `thumbv7em-none-eabihf` (Cortex-M4F) and `thumbv6m-none-eabi` (Cortex-M0) and
//!   passes its tests on a PC host.
//! - Fixed-size window of the last `N` samples, running sum and sum of squares.
//! - A sample is out of band when `|x - mean| > k * std` **and** `|x - mean| > min_dev`.
//!   The test is done in exact integer arithmetic (i128), with no square root:
//!   `(n*x - S)^2 * k_den^2 > k_num^2 * (n*Q - S^2)`.
//! - Out-of-band samples are not added to the window, so a fault does not
//!   teach the detector that faults are normal.
//! - After `M` consecutive out-of-band samples (default `M = N`), the signal is
//!   taken to have moved to a new level: the detector reports [`Verdict::Shift`],
//!   clears its window and learns again from that sample.
//!
//! ```
//! use nostd_sentinel::{Sentinel, Verdict};
//!
//! let mut s = Sentinel::<8>::new(4, 1, 2);
//! for _ in 0..8 {
//!     assert_eq!(s.observe(100), Verdict::Warmup);
//! }
//! assert_eq!(s.observe(101), Verdict::Normal);
//! assert_eq!(s.observe(5_000), Verdict::Anomaly);
//! ```
#![no_std]
#![forbid(unsafe_code)]

/// Largest window length accepted by [`Sentinel`]: `2^15`.
///
/// Overflow bound, with every sample in `[-2^31, 2^31)` and `n <= N <= 2^15`:
/// - `|S| <= N*2^31 <= 2^46` (fits the `i64` running sum),
/// - `|n*x - S| < N*2^32`, so `dev^2 < N^2*2^64 <= 2^94`,
///   and `dev^2 * k_den^2 < 2^94 * 2^32 = 2^126` (`k_den: u16`),
/// - `Q <= N*2^62`, so `0 <= n*Q - S^2 <= n*Q <= 2^92`,
///   and `k_num^2 * (n*Q - S^2) < 2^32 * 2^92 = 2^124` (`k_num: u16`),
/// - `n * min_dev < 2^15 * 2^32 = 2^47` (`min_dev: u32`).
///
/// Every intermediate stays below `i128::MAX = 2^127 - 1`.
pub const MAX_WINDOW: usize = 1 << 15;

/// Ring buffer of the last `N` samples with running sums.
#[derive(Clone, Debug)]
pub struct Window<const N: usize> {
    buf: [i32; N],
    len: usize,
    head: usize,
    sum: i64,
    sum_sq: i128,
}

impl<const N: usize> Window<N> {
    /// Empty window.
    pub const fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
            head: 0,
            sum: 0,
            sum_sq: 0,
        }
    }

    /// Adds `x`, dropping the oldest sample once the window is full.
    pub fn push(&mut self, x: i32) {
        // `head < N` always holds; `get_mut` instead of indexing keeps the bounds-check
        // panic path (and the formatting code it pulls in) out of the binary.
        let slot = match self.buf.get_mut(self.head) {
            Some(slot) => slot,
            None => return,
        };
        let old = core::mem::replace(slot, x);
        if self.len == N {
            self.sum -= old as i64;
            self.sum_sq -= (old as i128) * (old as i128);
        } else {
            self.len += 1;
        }
        // Conditional wrap instead of `% N`: no division (Cortex-M0 has no divider).
        self.head += 1;
        if self.head == N {
            self.head = 0;
        }
        self.sum += x as i64;
        self.sum_sq += (x as i128) * (x as i128);
    }

    /// Forgets every sample.
    pub fn clear(&mut self) {
        self.len = 0;
        self.head = 0;
        self.sum = 0;
        self.sum_sq = 0;
    }

    /// Number of samples held, at most `N`.
    pub fn len(&self) -> usize {
        self.len
    }

    /// `true` when no sample is held.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// `true` when `N` samples are held.
    pub fn is_full(&self) -> bool {
        self.len == N
    }

    /// Integer mean, rounded toward zero. `None` while empty.
    pub fn mean(&self) -> Option<i64> {
        if self.len == 0 {
            None
        } else {
            Some(self.sum / self.len as i64)
        }
    }

    /// `n^2 * variance = n*Q - S^2`, exact and never negative.
    fn scaled_variance(&self) -> i128 {
        let n = self.len as i128;
        n * self.sum_sq - (self.sum as i128) * (self.sum as i128)
    }
}

impl<const N: usize> Default for Window<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of [`Sentinel::observe`] for one sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The window is not full yet: no judgement.
    Warmup,
    /// Within band; the sample was added to the window.
    Normal,
    /// Out of band; the sample was not added to the window.
    Anomaly,
    /// `M`-th out-of-band sample in a row: the window was cleared and warm-up
    /// restarted from this sample, so the next `N - 1` verdicts are `Warmup`.
    Shift,
}

/// Anomaly detector over a window of `N` samples, `2 <= N <= MAX_WINDOW`
/// (checked at compile time).
///
/// ```compile_fail
/// let s = nostd_sentinel::Sentinel::<32_769>::new(4, 1, 2);
/// ```
#[derive(Clone, Debug)]
pub struct Sentinel<const N: usize> {
    window: Window<N>,
    k_num: u16,
    k_den: u16,
    min_dev: u32,
    relearn_after: usize,
    run: usize,
}

impl<const N: usize> Sentinel<N> {
    const N_IN_BOUNDS: () = assert!(N >= 2 && N <= MAX_WINDOW, "N must be in 2..=MAX_WINDOW");

    /// Threshold `k = k_num / k_den` standard deviations (`k_den > 0`); `min_dev` is an
    /// absolute floor in sensor units (stops a perfectly constant signal from flagging
    /// 1-unit jitter). Re-learning after `N` consecutive out-of-band samples is on by
    /// default; see [`Sentinel::relearn_after`].
    pub const fn new(k_num: u16, k_den: u16, min_dev: u32) -> Self {
        let () = Self::N_IN_BOUNDS;
        assert!(k_den > 0, "k_den must be > 0");
        Self {
            window: Window::new(),
            k_num,
            k_den,
            min_dev,
            relearn_after: N,
            run: 0,
        }
    }

    /// Sets `M`: the `M`-th consecutive out-of-band sample is reported as
    /// [`Verdict::Shift`] and the detector learns again from it. A burst shorter than
    /// `M` is reported as `Anomaly` and never learned. `0` disables re-learning
    /// (the 0.1 behaviour: a permanent level change is flagged forever).
    pub const fn relearn_after(mut self, m: usize) -> Self {
        self.relearn_after = m;
        self
    }

    /// Judges `x` against the current window, then updates the window.
    pub fn observe(&mut self, x: i32) -> Verdict {
        if !self.window.is_full() {
            self.window.push(x);
            return Verdict::Warmup;
        }
        // Cannot overflow: see `MAX_WINDOW`.
        let n = self.window.len() as i128;
        let dev = n * x as i128 - self.window.sum as i128; // n * (x - mean)
        let k_den = self.k_den as i128;
        let k_num = self.k_num as i128;
        let lhs = dev * dev * k_den * k_den;
        let rhs = k_num * k_num * self.window.scaled_variance();
        let beyond_floor = dev.abs() > n * self.min_dev as i128;
        if !(lhs > rhs && beyond_floor) {
            self.run = 0;
            self.window.push(x);
            return Verdict::Normal;
        }
        self.run = self.run.saturating_add(1);
        if self.run == self.relearn_after {
            self.run = 0;
            self.window.clear();
            self.window.push(x);
            Verdict::Shift
        } else {
            Verdict::Anomaly
        }
    }

    /// The current window.
    pub fn window(&self) -> &Window<N> {
        &self.window
    }
}
