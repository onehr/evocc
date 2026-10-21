/* Several declarators: the specifiers take the context of the first
 * declarator at file scope and of the function in a block. */
typedef int gone_t __attribute__((availability(macos,unavailable)));
gone_t a1, b1 __attribute__((unavailable));
gone_t a2 __attribute__((unavailable)), b2;
gone_t a3 __attribute__((unavailable)), b3 __attribute__((unavailable));
__attribute__((unavailable)) gone_t a4, b4;
int gone(void) __attribute__((availability(macos,unavailable)));
int c1 = sizeof(gone()), d1 __attribute__((unavailable)) = sizeof(gone());
void use(void) { gone_t x __attribute__((unavailable)); }
