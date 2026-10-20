// flags: --emit=asm --target=aarch64-apple-darwin -w
int placed(void) __attribute__((section(".text.custom")));
int placed(void) { return 0; }
