//! Reference cases with known answers, including blank controls that must give zero anomalies.
use nostd_sentinel::{Sentinel, Verdict};

/// Deterministic triangle wave between -100 and +100, period 40.
fn triangle(i: usize) -> i32 {
    let p = (i % 40) as i32;
    if p < 20 { -100 + 10 * p } else { 100 - 10 * (p - 20) }
}

fn count_anomalies<const N: usize>(s: &mut Sentinel<N>, xs: impl Iterator<Item = i32>) -> (usize, Vec<usize>) {
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
    let (n, _) = count_anomalies(&mut s, std::iter::repeat(1234).take(10_000));
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
    let xs = (0..10_000).map(|i| if i == spike_at { triangle(i) + 2_000 } else { triangle(i) });
    let (n, idx) = count_anomalies(&mut s, xs);
    assert_eq!(n, 1);
    assert_eq!(idx, vec![spike_at]);
}

#[test]
fn warmup_never_judges() {
    let mut s = Sentinel::<8>::new(4, 1, 0);
    for i in 0..8 {
        assert_eq!(s.observe(if i == 3 { 1_000_000 } else { 0 }), Verdict::Warmup);
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
