//! Integer-only, allocation-free anomaly detector for sensor streams.
//!
//! - `no_std`, no heap, no floating point. Checked: builds for `thumbv7em-none-eabihf`
//!   (Cortex-M4F) and passes its tests on a PC host.
//! - Fixed-size window of the last `N` samples, running sum and sum of squares.
//! - A sample is an anomaly when `|x - mean| > k * std` **and** `|x - mean| > min_dev`.
//!   The test is done in exact integer arithmetic (i128), with no square root:
//!   `(n*x - S)^2 * k_den^2 > k_num^2 * (n*Q - S^2)`.
//! - Anomalous samples are not added to the window, so a fault does not
//!   teach the detector that faults are normal.
#![no_std]

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
    pub const fn new() -> Self {
        Self { buf: [0; N], len: 0, head: 0, sum: 0, sum_sq: 0 }
    }

    pub fn push(&mut self, x: i32) {
        if self.len == N {
            let old = self.buf[self.head];
            self.sum -= old as i64;
            self.sum_sq -= (old as i128) * (old as i128);
        } else {
            self.len += 1;
        }
        self.buf[self.head] = x;
        self.head = (self.head + 1) % N;
        self.sum += x as i64;
        self.sum_sq += (x as i128) * (x as i128);
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.len == N
    }

    /// Integer mean, rounded toward zero. `None` while empty.
    pub fn mean(&self) -> Option<i64> {
        if self.len == 0 { None } else { Some(self.sum / self.len as i64) }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The window is not full yet: no judgement.
    Warmup,
    Normal,
    Anomaly,
}

/// Anomaly detector over a window of `N` samples.
#[derive(Clone, Debug)]
pub struct Sentinel<const N: usize> {
    window: Window<N>,
    k_num: i64,
    k_den: i64,
    min_dev: i64,
}

impl<const N: usize> Sentinel<N> {
    /// Threshold `k = k_num / k_den` standard deviations; `min_dev` is an absolute floor
    /// in sensor units (stops a perfectly constant signal from flagging 1-unit jitter).
    pub const fn new(k_num: i64, k_den: i64, min_dev: i64) -> Self {
        assert!(N > 1 && k_num >= 0 && k_den > 0 && min_dev >= 0);
        Self { window: Window::new(), k_num, k_den, min_dev }
    }

    pub fn observe(&mut self, x: i32) -> Verdict {
        if !self.window.is_full() {
            self.window.push(x);
            return Verdict::Warmup;
        }
        let n = self.window.len() as i128;
        let dev = n * x as i128 - self.window.sum as i128; // n * (x - mean)
        let lhs = dev * dev * (self.k_den as i128) * (self.k_den as i128);
        let rhs = (self.k_num as i128) * (self.k_num as i128) * self.window.scaled_variance();
        let beyond_floor = dev.abs() > n * self.min_dev as i128;
        if lhs > rhs && beyond_floor {
            Verdict::Anomaly
        } else {
            self.window.push(x);
            Verdict::Normal
        }
    }

    pub fn window(&self) -> &Window<N> {
        &self.window
    }
}
