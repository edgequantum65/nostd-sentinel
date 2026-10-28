# nostd-sentinel

[![tests](https://github.com/edgequantum65/nostd-sentinel/actions/workflows/tests.yml/badge.svg)](https://github.com/edgequantum65/nostd-sentinel/actions/workflows/tests.yml)

**Integer-only, allocation-free anomaly detector for sensor streams on microcontrollers.**

- `#![no_std]`, no heap, no floating point, no dependencies.
- Fixed window of the last `N` samples (const generic), running sum and sum of squares.
- A sample is an **anomaly** when it is more than `k` standard deviations from the window mean **and** more than `min_dev` sensor units away. `k` is a fraction `k_num / k_den`.
- The test is exact integer arithmetic, with no square root and no rounding drift: `(n·x − S)² · k_den² > k_num² · (n·Q − S²)`.
- Anomalous samples are **not** added to the window, so a fault does not teach the detector that faults are normal.

Edge Quantum 65 — R&D lab, Lannemezan, France. Rule of the lab: nothing is published that was not measured.

```rust
use nostd_sentinel::{Sentinel, Verdict};

// 64-sample window, 4 standard deviations, ignore deviations of 2 units or less
let mut sentinel = Sentinel::<64>::new(4, 1, 2);

loop {
    let x: i32 = read_sensor();
    if sentinel.observe(x) == Verdict::Anomaly {
        // raise a flag, light a LED, log the index...
    }
}
```

## Reference cases (`tests/reference_cases.rs`)

| Case | Expected | Result |
|---|---|---|
| Blank control: constant signal, 10 000 samples | 0 anomalies | 0 |
| Blank control: triangle wave, 10 000 samples | 0 anomalies | 0 |
| One spike of +2 000 at index 5 000 on the triangle wave | exactly 1, at 5 000 | 1, at 5 000 |
| Warm-up: no judgement before the window is full | `Warmup` × 8 | ok |
| Alternating `i32::MAX` / `i32::MIN` | no overflow, no panic | ok |
| Mean of [10, 20, 30, 40] | 25 | 25 |

## What was checked, and what was not

- Checked (28/10/2026): `cargo test` passes 6/6 on a PC host, and `cargo build --release --target thumbv7em-none-eabihf` (Cortex-M4F) succeeds.
- **Not yet measured**: flash and RAM footprint, cycles per sample on real hardware, other targets (Cortex-M0, RISC-V). These numbers will be published once measured on a board, not before.

```bash
cargo test
cargo build --release --target thumbv7em-none-eabihf
```

---

**FR** — Détecteur d'anomalies pour capteurs, en Rust `no_std` : entiers seulement, sans allocation, sans flottants. Témoins : un signal constant et un signal périodique donnent zéro anomalie ; un pic unique est trouvé une seule fois, au bon endroit. Compile pour Cortex-M4F ; empreinte mémoire et temps de calcul sur carte : pas encore mesurés, donc pas encore publiés.

License: MIT.
