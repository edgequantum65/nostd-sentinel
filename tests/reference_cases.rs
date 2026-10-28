//! Reference cases with known answers, including blank controls that must give zero anomalies.
use nostd_sentinel::{Sentinel, Verdict, MAX_WINDOW};

/// Deterministic triangle wave between -100 and +100, period 40.
fn triangle(i: usize) -> i32 {
    let p = (i % 40) as i32;
    if p < 20 {
        -100 + 10 * p
    } else {
        100 - 10 * (p - 20)
    }
}

/// Small seeded linear congruential generator (Knuth MMIX constants), no dependency.
struct Lcg(u64);

impl Lcg {
    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 32) as u32
    }

    /// In `0..m` (`m > 0`), up to a negligible modulo bias.
    fn below(&mut self, m: u64) -> u64 {
        self.next_u32() as u64 % m
    }
}

fn count_anomalies<const N: usize>(
    s: &mut Sentinel<N>,
    xs: impl Iterator<Item = i32>,
) -> (usize, Vec<usize>) {
    let mut idx = Vec::new();
    for (i, x) in xs.enumerate() {
        if s.observe(x) == Verdict::Anomaly {
            idx.push(i);
        }
    }
    (idx.len(), idx)
}

#[test]
fn blank_constant_signal_gives_zero() {
    let mut s = Sentinel::<32>::new(4, 1, 2);
    let (n, _) = count_anomalies(&mut s, (0..10_000).map(|_| 1234));
    assert_eq!(n, 0);
}

#[test]
fn blank_periodic_signal_gives_zero() {
    let mut s = Sentinel::<64>::new(4, 1, 2);
    let (n, _) = count_anomalies(&mut s, (0..10_000).map(triangle));
    assert_eq!(n, 0);
}

#[test]
fn single_spike_is_found_exactly_once_at_its_index() {
    let mut s = Sentinel::<64>::new(4, 1, 2);
    let spike_at = 5_000;
    let xs = (0..10_000).map(|i| {
        if i == spike_at {
            triangle(i) + 2_000
        } else {
            triangle(i)
        }
    });
    let (n, idx) = count_anomalies(&mut s, xs);
    assert_eq!(n, 1);
    assert_eq!(idx, vec![spike_at]);
}

#[test]
fn warmup_never_judges() {
    let mut s = Sentinel::<8>::new(4, 1, 0);
    for i in 0..8 {
        assert_eq!(
            s.observe(if i == 3 { 1_000_000 } else { 0 }),
            Verdict::Warmup
        );
    }
}

#[test]
fn extreme_values_do_not_overflow() {
    let mut s = Sentinel::<16>::new(3, 1, 0);
    for i in 0..1_000 {
        let x = if i % 2 == 0 { i32::MAX } else { i32::MIN };
        let _ = s.observe(x);
    }
}

#[test]
fn mean_is_exact_on_known_window() {
    let mut s = Sentinel::<4>::new(4, 1, 0);
    for x in [10, 20, 30, 40] {
        s.observe(x);
    }
    assert_eq!(s.window().mean(), Some(25));
}

/// Level 0 for 5 000 samples, then level +500 for 5 000 samples, jitter in [-3, 3].
fn level_shift(i: usize, rng: &mut Lcg) -> i32 {
    let level = if i < 5_000 { 0 } else { 500 };
    level + rng.below(7) as i32 - 3
}

#[test]
fn level_shift_is_relearned_after_n_anomalies() {
    let mut s = Sentinel::<64>::new(4, 1, 2);
    let mut rng = Lcg(12_345);
    let verdicts: Vec<Verdict> = (0..10_000)
        .map(|i| s.observe(level_shift(i, &mut rng)))
        .collect();
    // Before the step: 64 warm-up samples, then only Normal.
    assert!(verdicts[..64].iter().all(|v| *v == Verdict::Warmup));
    assert!(verdicts[64..5_000].iter().all(|v| *v == Verdict::Normal));
    // After the step: 63 Anomaly, 1 Shift, 63 Warmup, then only Normal.
    let mut expected = vec![Verdict::Anomaly; 63];
    expected.push(Verdict::Shift);
    expected.extend(vec![Verdict::Warmup; 63]);
    expected.extend(vec![Verdict::Normal; 5_000 - 127]);
    assert_eq!(&verdicts[5_000..], &expected[..]);
}

#[test]
fn level_shift_without_relearning_is_flagged_forever() {
    let mut s = Sentinel::<64>::new(4, 1, 2).relearn_after(0);
    let mut rng = Lcg(12_345);
    let xs = (0..10_000).map(|i| level_shift(i, &mut rng));
    let (n, idx) = count_anomalies(&mut s, xs);
    assert_eq!(n, 5_000);
    assert_eq!(idx[0], 5_000);
}

#[test]
fn burst_shorter_than_m_stays_anomalous_and_is_not_learned() {
    const M: usize = 16;
    let burst = 5_000..5_000 + M - 1;
    let mut s = Sentinel::<64>::new(4, 1, 2).relearn_after(M);
    // Twin that never sees the burst: both must agree everywhere else.
    let mut twin = Sentinel::<64>::new(4, 1, 2).relearn_after(M);
    let mut anomalies = Vec::new();
    for i in 0..10_000 {
        let x = triangle(i);
        if burst.contains(&i) {
            assert_eq!(s.observe(x + 2_000), Verdict::Anomaly);
            anomalies.push(i);
            continue;
        }
        assert_eq!(s.observe(x), twin.observe(x));
        assert_eq!(s.window().mean(), twin.window().mean());
        assert_eq!(s.window().len(), twin.window().len());
    }
    assert_eq!(anomalies, burst.collect::<Vec<_>>());
}

#[test]
fn burst_of_exactly_m_triggers_shift() {
    const M: usize = 16;
    let mut s = Sentinel::<64>::new(4, 1, 2).relearn_after(M);
    for i in 0..1_000 {
        s.observe(triangle(i));
    }
    for _ in 0..M - 1 {
        assert_eq!(s.observe(5_000), Verdict::Anomaly);
    }
    assert_eq!(s.observe(5_000), Verdict::Shift);
    assert_eq!(s.window().len(), 1);
    for _ in 0..63 {
        assert_eq!(s.observe(5_000), Verdict::Warmup);
    }
    assert_eq!(s.observe(5_000), Verdict::Normal);
}

/// Jitter in [-3, 3], used to fill a window.
fn jitter(i: usize) -> i32 {
    (i % 7) as i32 - 3
}

fn verdicts<const N: usize>(s: &mut Sentinel<N>, xs: &[i32]) -> Vec<Verdict> {
    xs.iter().map(|&x| s.observe(x)).collect()
}

#[test]
fn run_resets_on_normal() {
    use Verdict::*;
    let mut s = Sentinel::<8>::new(4, 1, 2).relearn_after(3);
    for i in 0..8 {
        s.observe(jitter(i));
    }
    let v = verdicts(&mut s, &[1000, 1000, 0, 1000, 1000, 0, 1000, 1000, 1000]);
    assert_eq!(
        v,
        [Anomaly, Anomaly, Normal, Anomaly, Anomaly, Normal, Anomaly, Anomaly, Shift]
    );
}

#[test]
fn run_counts_one_side_of_the_mean_only() {
    use Verdict::*;
    let mut s = Sentinel::<8>::new(4, 1, 2).relearn_after(3);
    for i in 0..8 {
        s.observe(jitter(i));
    }
    let v = verdicts(&mut s, &[1000, 1000, -1000, -1000, -1000]);
    assert_eq!(v, [Anomaly, Anomaly, Anomaly, Anomaly, Shift]);
}

#[test]
fn window_after_shift_is_fresh_and_second_shift_works() {
    let mut s = Sentinel::<8>::new(4, 1, 2);
    for i in 0..8 {
        s.observe(jitter(i));
    }
    for _ in 0..7 {
        assert_eq!(s.observe(5_000), Verdict::Anomaly);
    }
    assert_eq!(s.observe(5_000), Verdict::Shift);
    for _ in 0..7 {
        assert_eq!(s.observe(5_000), Verdict::Warmup);
    }
    // Window is now 8 x 5 000 (zero variance): 3 units above it is out of band.
    for _ in 0..7 {
        assert_eq!(s.observe(5_003), Verdict::Anomaly);
    }
    assert_eq!(s.observe(5_003), Verdict::Shift);
}

/// Verdicts from index 100 on, for jitter with a +500 fault over `100..100 + len`.
fn fault_trace(len: usize) -> Vec<Verdict> {
    let mut s = Sentinel::<16>::new(4, 1, 2);
    let xs: Vec<i32> = (0..400)
        .map(|i| {
            jitter(i)
                + if (100..100 + len).contains(&i) {
                    500
                } else {
                    0
                }
        })
        .collect();
    verdicts(&mut s, &xs).split_off(100)
}

/// `n` Anomaly, Shift, `n` Warmup (`N = M = 16` gives `n = 15`).
fn shift_then_warmup() -> Vec<Verdict> {
    let mut v = vec![Verdict::Anomaly; 15];
    v.push(Verdict::Shift);
    v.extend(vec![Verdict::Warmup; 15]);
    v
}

#[test]
fn fault_of_m_or_more_is_learned() {
    // Ends during the warm-up after the Shift: the return is learned, not reported.
    let mut expected = shift_then_warmup();
    expected.extend(vec![Verdict::Normal; 300 - 31]);
    assert_eq!(fault_trace(20), expected);
    // Outlasts the warm-up: the return is judged, and is a level shift of its own.
    let mut expected = shift_then_warmup();
    expected.extend(vec![Verdict::Normal; 100 - 31]);
    expected.extend(shift_then_warmup());
    expected.extend(vec![Verdict::Normal; 200 - 31]);
    assert_eq!(fault_trace(100), expected);
}

#[test]
fn fault_alternating_around_the_mean_never_shifts() {
    let mut s = Sentinel::<16>::new(4, 1, 2);
    for i in 0..16 {
        assert_eq!(s.observe(jitter(i)), Verdict::Warmup);
    }
    for i in 0..10_000 {
        let x = if i % 2 == 0 { 100_000 } else { -100_000 };
        assert_eq!(s.observe(x), Verdict::Anomaly);
    }
}

#[test]
#[should_panic]
fn relearn_after_one_is_rejected() {
    let _ = Sentinel::<8>::new(4, 1, 2).relearn_after(1);
}

/// Fresh detector, window filled with `fill`, then judges `x`.
fn judge<F: Fn(usize) -> i32, const N: usize>(
    k_num: u16,
    k_den: u16,
    min_dev: u32,
    fill: F,
    x: i32,
) -> Verdict {
    let mut s = Sentinel::<N>::new(k_num, k_den, min_dev).relearn_after(0);
    for i in 0..N {
        assert_eq!(s.observe(fill(i)), Verdict::Warmup);
    }
    s.observe(x)
}

#[test]
fn extreme_values_with_maximal_parameters_are_exact() {
    const W: usize = MAX_WINDOW;
    let max = u16::MAX;
    // Constant i32::MIN window, x = i32::MAX: |x - mean| = 2^32 - 1, the largest
    // possible deviation; k_den = u16::MAX gives the largest left-hand side (~2^126).
    for (k_num, k_den) in [(max, 1), (max, max), (1, max), (0, max)] {
        let v = judge::<_, W>(k_num, k_den, u32::MAX - 1, |_| i32::MIN, i32::MAX);
        assert_eq!(v, Verdict::Anomaly);
        // The floor is not exceeded: 2^32 - 1 > u32::MAX is false.
        let v = judge::<_, W>(k_num, k_den, u32::MAX, |_| i32::MIN, i32::MAX);
        assert_eq!(v, Verdict::Normal);
    }
    // Alternating i32::MAX / i32::MIN window: largest variance, std = 2^31 - 1/2,
    // and x = i32::MAX is exactly 1 std away from the mean (-1/2).
    let alt = |i: usize| if i % 2 == 0 { i32::MAX } else { i32::MIN };
    assert_eq!(judge::<_, W>(max, max, 0, alt, i32::MAX), Verdict::Normal); // k = 1
    assert_eq!(judge::<_, W>(1, 1, 0, alt, i32::MAX), Verdict::Normal); // k = 1
    assert_eq!(
        judge::<_, W>(max - 1, max, 0, alt, i32::MAX),
        Verdict::Anomaly
    ); // k < 1
    assert_eq!(judge::<_, W>(1, max, 0, alt, i32::MAX), Verdict::Anomaly);
    assert_eq!(judge::<_, W>(max, 1, 0, alt, i32::MAX), Verdict::Normal);
}

/// Straightforward floating-point reference: `Some(is_anomaly)`, or `None` when the
/// sample is too close to a boundary for `f64` to be trusted.
fn reference(window: &[i32], x: i32, k_num: u16, k_den: u16, min_dev: u32) -> Option<bool> {
    let n = window.len() as f64;
    let mean = window.iter().map(|&v| v as i64).sum::<i64>() as f64 / n;
    let var = window
        .iter()
        .map(|&v| (v as f64 - mean) * (v as f64 - mean))
        .sum::<f64>()
        / n;
    let dev = (x as f64 - mean).abs();
    let thr = k_num as f64 / k_den as f64 * var.sqrt();
    let floor = min_dev as f64;
    let tol = 1e-9;
    if (dev - thr).abs() <= tol * (dev + thr + 1.0) || (dev - floor).abs() <= tol * (dev + 1.0) {
        return None;
    }
    Some(dev > thr && dev > floor)
}

/// Totals of a cross-check run.
#[derive(Debug, Default)]
struct Tally {
    anomalies: usize,
    normals: usize,
    /// Cases stopped at a sample within `f64` tolerance of a boundary.
    stopped: usize,
}

/// Random streams at several scales: `N` warm-up samples, `0..=2N` more samples drawn
/// the same way (so the window slides and old values are subtracted), then one sample
/// anywhere. Every judged sample is compared with the `f64` reference.
fn cross_check<const N: usize>(rng: &mut Lcg, cases: usize, tally: &mut Tally) {
    let clamp = |v: i64| v.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    for _ in 0..cases {
        let scale: i64 = [3, 1_000, 1 << 20, 1 << 31][rng.below(4) as usize];
        let base = rng.next_u32() as i32 as i64 / 2;
        let constant = rng.below(20) == 0;
        // A constant window only gets the final sample: more samples equal to the
        // mean would sit exactly on the boundary (|x - mean| = k * std = 0).
        let len = if constant {
            N
        } else {
            N + rng.below(2 * N as u64 + 1) as usize
        };
        let mut stream = Vec::with_capacity(len + 1);
        for i in 0..len {
            let noise = if constant && i > 0 {
                0
            } else {
                rng.below(2 * scale as u64 + 1) as i64 - scale
            };
            stream.push(clamp(base + noise));
        }
        stream.push(if rng.below(4) == 0 {
            rng.next_u32() as i32
        } else {
            let step = rng.below(9 * scale as u64 + 1) as i64;
            clamp(if rng.below(2) == 0 {
                base - step
            } else {
                base + step
            })
        });
        let k_den = [1, 2, 3, 10, 100, u16::MAX, 1 + rng.below(65_535) as u16];
        let k_den = k_den[rng.below(7) as usize];
        let k_num = rng.below((6 * k_den as u64 + 1).min(65_536)) as u16;
        let min_dev = if rng.below(2) == 0 {
            0
        } else {
            rng.below(scale as u64) as u32
        };

        let mut s = Sentinel::<N>::new(k_num, k_den, min_dev).relearn_after(0);
        let mut window: Vec<i32> = Vec::with_capacity(N);
        for &x in &stream {
            let got = s.observe(x);
            if window.len() < N {
                assert_eq!(got, Verdict::Warmup);
                window.push(x);
                continue;
            }
            let expected = match reference(&window, x, k_num, k_den, min_dev) {
                None => {
                    tally.stopped += 1;
                    break;
                }
                Some(true) => {
                    tally.anomalies += 1;
                    Verdict::Anomaly
                }
                Some(false) => {
                    tally.normals += 1;
                    window.remove(0);
                    window.push(x);
                    Verdict::Normal
                }
            };
            assert_eq!(
                got, expected,
                "window {:?} x {} k {}/{} floor {}",
                window, x, k_num, k_den, min_dev
            );
        }
    }
}

#[test]
fn randomized_cross_check_against_f64_reference() {
    let mut rng = Lcg(20_261_028);
    let mut t = Tally::default();
    cross_check::<2>(&mut rng, 5_000, &mut t);
    cross_check::<3>(&mut rng, 5_000, &mut t);
    cross_check::<16>(&mut rng, 5_000, &mut t);
    cross_check::<64>(&mut rng, 5_000, &mut t);
    cross_check::<257>(&mut rng, 5_000, &mut t);
    println!(
        "cross-check: 25000 streams, {} judged samples agree ({} anomalies, {} normals), \
         {} streams stopped near a boundary",
        t.anomalies + t.normals,
        t.anomalies,
        t.normals,
        t.stopped
    );
    assert!(t.anomalies > 10_000 && t.normals > 100_000, "{:?}", t);
    assert!(t.stopped < 2_500, "{:?}", t);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn ram_footprint_is_known() {
    // Computed by the compiler for this host (x86_64), not measured on a board.
    // i128 is 16-byte aligned on x86_64 since Rust 1.77 (8-byte before): 336 vs 328.
    let size = core::mem::size_of::<Sentinel<64>>();
    println!("size_of::<Sentinel<64>>() = {}", size);
    let expected = if core::mem::align_of::<i128>() == 16 {
        336
    } else {
        328
    };
    assert_eq!(size, expected);
}
