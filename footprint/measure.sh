#!/bin/sh
# Flash size of `Sentinel::<64>::observe` (everything linked from it), no board needed.
# Needs: rustup target add thumbv7em-none-eabihf thumbv6m-none-eabi
#        rustup component add llvm-tools
set -eu
cd "$(dirname "$0")"
rustc -V
BIN="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin"
for target in thumbv7em-none-eabihf thumbv6m-none-eabi; do
    for opt in 3 s; do
        CARGO_PROFILE_RELEASE_OPT_LEVEL=$opt cargo rustc -q --release --target "$target" \
            -- -C link-arg=--entry=sentinel64_observe
        elf="target/$target/release/sentinel-footprint"
        text=$("$BIN/llvm-size" -A "$elf" | awk '$1 == ".text" { print $2 }')
        echo "$target opt-level=$opt: .text = $text bytes"
        "$BIN/llvm-nm" --size-sort --print-size --radix=d "$elf" | sed 's/^/    /'
        if "$BIN/llvm-nm" "$elf" | grep -q panic; then echo "    (panic code linked)"; fi
    done
done
