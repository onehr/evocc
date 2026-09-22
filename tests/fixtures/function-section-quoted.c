/* GNU-as section spelling control; inspect names, not only exit status. */
int backslash(void) __attribute__((section(".text.a\\b")));
int backslash(void) { return 3; }
int quote(void) __attribute__((section(".text.a\"b")));
int quote(void) { return 4; }
int main(void) { return backslash() + quote() != 7; }
