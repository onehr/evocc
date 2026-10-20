#!/bin/sh
# Test checker for proof-required compilation. It changes a proof input and
# then runs the real Lean checker. EVOCC_TEST_LEAN_CHECKER names the real
# checker. EVOCC_TEST_ACTION selects the change.
stage=$2
for binding in "$@"; do
    case "$binding" in
        source=*) inputs=$(dirname "${binding#source=}") ;;
        ir=*) ir=${binding#ir=}; inputs=$(dirname "$ir") ;;
    esac
done
case "$EVOCC_TEST_ACTION" in
    change-ir)
        # abs returns its argument instead of the absolute value.
        sed 's/ret %v9/ret %v8/' "$ir" > "$ir.changed" && mv "$ir.changed" "$ir"
        ;;
    remove-ssa)
        rm -f "$inputs/ssa-evomir.json"
        ;;
    block-record)
        # A dangling link makes create_new fail but is not an output record.
        # Keep the assembly path free to expose a partial publication.
        if [ ! -L "$inputs/../record.json" ]; then
            ln -s "$inputs/missing-record" "$inputs/../record.json"
        fi
        ;;
    change-ir-after-acceptance)
        if [ "$stage" = lowering ]; then
            "$EVOCC_TEST_LEAN_CHECKER" "$@"
            status=$?
            echo "" >> "$ir"
            exit $status
        fi
        ;;
esac
exec "$EVOCC_TEST_LEAN_CHECKER" "$@"
