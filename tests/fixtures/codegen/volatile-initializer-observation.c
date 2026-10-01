/* Pending source-bridge discrepancy: C-HIR currently omits the volatile
 * initializer store event while EvoIR lowering emits it. Both return zero.
 * Keep this input unchanged; do not remove events or relabel it TV-admitted.
 * The N02 pass domain excludes volatile operations. */
int main(void) {
    volatile unsigned value = 7;
    return value != 7;
}
