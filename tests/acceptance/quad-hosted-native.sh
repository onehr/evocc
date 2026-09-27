#!/bin/bash
# Run inside the pinned ARM64 lab, from the repository root. The supplied
# directory contains host-emitted subject assembly and qualified oracle streams.
set -eu
quad_dir=${1:?prepared binary128 test directory required}
sha256sum "$quad_dir/evocc-quad-subject"
for cc in gcc-14 clang-19; do
    for co in 0 2; do
        "$cc" -O"$co" tests/fixtures/floating-quad-raw-driver.c "$quad_dir/quad-raw-ir.s" -lm -o "$quad_dir/native-raw"
        "$quad_dir/native-raw"
        "$cc" -O"$co" -DEVO_QUAD_SUBJECT tests/fixtures/floating-quad-fma.c "$quad_dir/quad-fma-ir.s" -lm -o "$quad_dir/native-ir-fma"
        "$quad_dir/native-ir-fma" > "$quad_dir/native-ir-fma.txt"
        cmp "$quad_dir/quad-fma-gcc-O2.txt" "$quad_dir/native-ir-fma.txt"
        "$cc" -O"$co" tests/fixtures/floating-quad-abi-{source,caller,driver}.c -lm -o "$quad_dir/native-abi-control"
        "$quad_dir/native-abi-control"
        for eo in 0 2; do
            "$cc" -O"$co" tests/fixtures/floating-quad-core-driver.c "$quad_dir/quad-core-evo-O$eo.s" -lm -o "$quad_dir/native-core"
            "$quad_dir/native-core"
            for lane in formats smallints helpers fma conversions; do
                case $lane in
                    formats) source=format-source; reference=quad-formats-gcc-14-O0.txt;;
                    smallints) source=smallint-source; reference=quad-smallints-gcc-14-O0.txt;;
                    helpers) source=numeric-source; reference=quad-gcc-14-O0.txt;;
                    fma) source=numeric-source; reference=quad-fma-gcc-O2.txt;;
                    conversions) source=numeric-source; reference=quad-conversions.txt;;
                esac
                "$cc" -O"$co" -DEVO_QUAD_SUBJECT "tests/fixtures/floating-quad-$lane.c" "$quad_dir/quad-$source-evo-O$eo.s" -lm -o "$quad_dir/native-$lane"
                "$quad_dir/native-$lane" > "$quad_dir/native-$lane.txt"
                cmp "$quad_dir/$reference" "$quad_dir/native-$lane.txt"
            done
            "$cc" -O"$co" -DEVO_QUAD_PREDICATES tests/fixtures/floating-quad-comparisons.c "$quad_dir/quad-numeric-source-evo-O$eo.s" -lm -o "$quad_dir/native-predicates"
            "$quad_dir/native-predicates"
            "$cc" -O"$co" tests/fixtures/floating-quad-abi-caller.c tests/fixtures/floating-quad-abi-driver.c "$quad_dir/quad-abi-source-evo-O$eo.s" -lm -o "$quad_dir/native-abi-forward"
            "$quad_dir/native-abi-forward"
            "$cc" -O"$co" tests/fixtures/floating-quad-abi-source.c tests/fixtures/floating-quad-abi-driver.c "$quad_dir/quad-abi-caller-evo-O$eo.s" -lm -o "$quad_dir/native-abi-reverse"
            "$quad_dir/native-abi-reverse"
            echo "$cc O$co / EvoCC O$eo: core, five exact streams, predicates, both ABI directions PASS"
        done
    done
done
