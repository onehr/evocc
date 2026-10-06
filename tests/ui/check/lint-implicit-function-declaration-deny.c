// flags: --parse-only -Werror=implicit-function-declaration
// exit: 1
int f(void) { return missing(); }
