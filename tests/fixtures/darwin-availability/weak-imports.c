/* Weak imports (EvoIR 10.1.9): a function or object declaration that is
 * introduced after the deployment target, or marked weak_import, is a weak
 * reference unless the unit defines it, as in Apple clang 21. */
int later(void) __attribute__((availability(macos,introduced=13.0)));
int now(void) __attribute__((availability(macos,introduced=10.0)));
int wi(void) __attribute__((weak_import));
extern int later_object __attribute__((availability(macos,introduced=13.0)));
int defined_later(void) __attribute__((availability(macos,introduced=13.0)));
int defined_later(void) { return 3; }
int redeclared(void) __attribute__((availability(macos,introduced=14.0)));
int redeclared(void);
static int internal(void) __attribute__((availability(macos,introduced=13.0)));
static int internal(void) { return 4; }
int g(void) { return later() + now() + wi() + later_object + defined_later() + redeclared() + internal(); }
int h(void) { return later != 0; }
