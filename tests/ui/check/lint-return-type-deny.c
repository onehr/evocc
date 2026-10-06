// flags: --parse-only -Werror=return-type
// exit: 1
int f(int x) { if (x) return 1; }
