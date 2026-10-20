// flags: --emit=asm --target=aarch64-unknown-linux-gnu -w -mgeneral-regs-only
#include "backend-vector.h"
int local(void) { int x[4] = {1, 2, 3, 4}; return x[1]; }
