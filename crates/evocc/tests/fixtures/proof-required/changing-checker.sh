#!/bin/sh
# Test checker for proof-required compilation. It changes a proof input and
# then runs the real Lean checker. EVOCC_TEST_LEAN_CHECKER names the real
# checker. EVOCC_TEST_ACTION selects the change.
stage=$2
for binding in "$@"; do
    case "$binding" in
        ir=*) ir=${binding#ir=} ;;
    esac
done
inputs=$(dirname "$ir")
case "$EVOCC_TEST_ACTION" in
    change-ir)
        # abs returns its argument instead of the absolute value.
        sed 's/ret %v9/ret %v8/' "$ir" > "$ir.changed" && mv "$ir.changed" "$ir"
        ;;
    remove-ssa)
        rm -f "$inputs/ssa-evomir.json"
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
