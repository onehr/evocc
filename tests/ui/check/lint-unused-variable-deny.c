// flags: --parse-only -Werror=unused-variable
// exit: 1
int f(void) { int unused = 2; return 0; }
