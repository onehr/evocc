// flags: --emit=asm --target=aarch64-unknown-linux-gnu -w
#include "backend-vector.h"
int local(void) { v4u8 x = {1, 2, 3, 4}; return x[1]; }
