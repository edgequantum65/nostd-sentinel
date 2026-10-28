# nostd-sentinel

[![tests](https://github.com/edgequantum65/nostd-sentinel/actions/workflows/tests.yml/badge.svg)](https://github.com/edgequantum65/nostd-sentinel/actions/workflows/tests.yml)

**Integer-only, allocation-free anomaly detector for sensor streams on microcontrollers.**

- `#![no_std]`, no heap, no floating point, no `unsafe`, no dependencies.
- Fixed window of the last `N` samples (const generic), running sum and sum of squares.
- A sample is an **anomaly** when it is more than `k` standard deviations from the window mean **and** more than `min_dev` sensor units away. `k` is a fraction `k_num / k_den`.
- The test is exact integer arithmetic, with no square root and no rounding drift: `(n·x − S)² · k_den² > k_num² · (n·Q − S²)`.
- Anomalous samples are **not** added to the window, so a fault does not teach the detector that faults are normal.
- If the signal moves to a new level and stays there, the detector reports `Shift` and learns the new level (see below), instead of flagging every sample forever.

Edge Quantum 65 — R&D lab, Lannemezan, France. Rule of the lab: nothing is published that was not measured.

```rust
use nostd_sentinel::{Sentinel, Verdict};

// 64-sample window, 4 standard deviations, ignore deviations of 2 units or less
let mut sentinel = Sentinel::<64>::new(4, 1, 2);

loop {
    let x: i32 = read_sensor();
    match sentinel.observe(x) {
        Verdict::Anomaly => { /* raise a flag, light a LED, log the index... */ }
        Verdict::Shift => { /* the signal moved to a new level; learning it again */ }
        Verdict::Warmup | Verdict::Normal => {}
    }
}
```

## Parameters and their bounds

| Parameter | Type | Bound | Checked |
|---|---|---|---|
| `N` (window length) | `usize` | 2 ≤ `N` ≤ 32 768 (`MAX_WINDOW`) | at compile time |
| `k_num` | `u16` | any | by the type |
| `k_den` | `u16` | > 0 | `assert!` in `new` (at compile time in a `const`) |
| `min_dev` | `u32` | any | by the type |
| `M` (`relearn_after`) | `usize` | 0 = never, default `N` | — |

With these bounds every intermediate value fits in an `i128`: `|n·x − S| < N·2³²`, so the left-hand side stays below 2¹²⁶ and the right-hand side below 2¹²⁴ (derivation in the doc comment of `MAX_WINDOW`). Version 0.1 took `i64` thresholds and could overflow; the 0.2 types make that impossible.

## Level shifts: `Shift` and re-learning

A burst of out-of-band samples shorter than `M` is reported as `Anomaly`, sample by sample, and none of it is learned. The `M`-th out-of-band sample **in a row** is reported as `Shift`: the window is cleared and warm-up restarts from that sample, so the next `N − 1` verdicts are `Warmup`, then judging resumes on the new level.

- Default: `M = N` (one window length of evidence before believing the new level).
- `Sentinel::<64>::new(4, 1, 2).relearn_after(200)` waits longer; `.relearn_after(0)` never re-learns (the 0.1 behaviour).
- Trade-off: a fault that lasts `M` samples or more is learned as the new normal. Pick `M` longer than the longest fault you must keep flagging.

## Reference cases (`tests/reference_cases.rs`)

| Case | Expected | Result |
|---|---|---|
| Blank control: constant signal, 10 000 samples | 0 anomalies | 0 |
| Blank control: triangle wave, 10 000 samples | 0 anomalies | 0 |
| One spike of +2 000 at index 5 000 on the triangle wave | exactly 1, at 5 000 | 1, at 5 000 |
| Warm-up: no judgement before the window is full | `Warmup` × 8 | ok |
| Alternating `i32::MAX` / `i32::MIN` | no overflow, no panic | ok |
| Mean of [10, 20, 30, 40] | 25 | 25 |
| Level step 0 → +500 at sample 5 000, jitter ±3, `N` = 64, `k` = 4, `min_dev` = 2 | 63 `Anomaly`, 1 `Shift`, 63 `Warmup`, then only `Normal` | exact sequence, 0 anomalies after the `Shift` |
| Same step with `relearn_after(0)` | flagged forever | 5 000 anomalies out of 5 000 |
| Burst of +2 000 lasting 15 samples, `M` = 16 | 15 `Anomaly`, not learned | 15; afterwards same mean, length and verdicts as a run without the burst |
| Burst lasting exactly `M` = 16 samples | 15 `Anomaly`, then `Shift` | ok |
| `N` = 32 768, constant `i32::MIN` window, `x = i32::MAX`, `k_num`/`k_den` up to 65 535 | `Anomaly`; `Normal` when `min_dev = u32::MAX` | ok, debug and release |
| `N` = 32 768, alternating `i32::MAX` / `i32::MIN`, `x = i32::MAX` (exactly 1 std away) | `Normal` for `k` = 1, `Anomaly` for `k` = 65 534/65 535 | ok, debug and release |
| Random cross-check against a plain `f64` implementation, 25 000 cases, `N` ∈ {2, 3, 16, 64, 257} | same verdict | 25 000 / 25 000 agree (19 216 anomalies, 5 742 normal; 42 cases within 10⁻⁹ of a boundary not compared) |
| `N` = 32 769 | does not compile | does not compile (doc test) |

## Footprint

Computed by the compiler or the linker, **not measured on a board**.

- **RAM** (`core::mem::size_of::<Sentinel<64>>()`): 304 bytes on `thumbv7em-none-eabihf` and `thumbv6m-none-eabi` (compile-time assertion in `footprint/`); 336 bytes on an x86_64 PC with rustc ≥ 1.77 (320 with older rustc, where `i128` was 8-byte aligned). The detector holds no other state: no `static`, no heap.
- **Flash**: `.text` of a program that contains only `Sentinel::<64>::observe` and what it calls, obtained with [`footprint/measure.sh`](footprint/measure.sh) (rustc 1.97.0, `panic = "abort"`, `codegen-units = 1`). No panic code is linked.

| Target | `opt-level = 3` | `opt-level = "s"` |
|---|---|---|
| `thumbv7em-none-eabihf` (Cortex-M4F) | 1 172 bytes | 976 bytes |
| `thumbv6m-none-eabi` (Cortex-M0) | 2 052 bytes | 1 558 bytes |

## What was checked, and what was not

- Checked (28/10/2026), rustc 1.97.0: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` (clean), `cargo test` and `cargo test --release` (13/13 reference cases + 2 doc tests), `cargo build --release` for `thumbv7em-none-eabihf` (Cortex-M4F) and `thumbv6m-none-eabi` (Cortex-M0). The same tests also pass with rustc 1.57.0, the declared minimum version (1.56 is refused: it cannot `assert!` in a `const fn`).
- **Not yet measured**: cycles per sample, behaviour on a real board, real sensor data, other targets (RISC-V). These numbers will be published once measured on a board, not before.

```bash
cargo test
cargo test --release
cargo build --release --target thumbv7em-none-eabihf
cargo build --release --target thumbv6m-none-eabi
footprint/measure.sh
```

---

**FR** — Détecteur d'anomalies pour capteurs, en Rust `no_std` : entiers seulement, sans allocation, sans flottants, sans `unsafe`. Témoins : un signal constant et un signal périodique donnent zéro anomalie ; un pic unique est trouvé une seule fois, au bon endroit. Version 0.2 : après `M` échantillons hors bande consécutifs (par défaut `M = N`), le détecteur signale `Shift` et réapprend le nouveau niveau, au lieu de signaler une anomalie à chaque échantillon pour toujours ; les seuils sont typés (`u16`, `u32`, `N` ≤ 32 768) pour qu'aucun calcul ne puisse déborder ; 25 000 cas tirés au hasard donnent le même verdict qu'une implémentation en flottants. Compile pour Cortex-M4F et Cortex-M0 ; RAM 304 octets pour `N = 64` (calculée par le compilateur) ; flash de `observe` : 1 172 octets (M4F) et 2 052 octets (M0) en `opt-level = 3`, mesurés à l'édition de liens, pas sur carte. Temps de calcul sur carte : pas encore mesuré, donc pas encore publié.

License: MIT.
