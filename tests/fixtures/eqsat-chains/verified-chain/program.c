/* R01a: ordinary lowering leaves identity roots for the shared proof chain. */
unsigned simplify(unsigned x) { return (x + 0u) * 1u; }
int main(void) { return simplify(17u) != 17u; }
