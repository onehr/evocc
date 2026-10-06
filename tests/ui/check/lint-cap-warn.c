// flags: --parse-only -Werror=unused-variable --cap-lints=warn
// exit: 0
int f(void) { int unused = 2; return 0; }
