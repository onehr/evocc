/* The guard pattern of <secure/_string.h> and <secure/_stdio.h> in the
 * macOS 15 SDK. A GNU compiler takes the checked macros even when
 * __has_builtin does not report the builtins, so the test does not depend
 * on an installed SDK. */
#ifndef GUARD15_H
#define GUARD15_H

typedef unsigned long size_t;
typedef __builtin_va_list va_list;
#define va_start(list, last) __builtin_va_start(list, last)
#define va_end(list) __builtin_va_end(list)

void *memcpy(void *, const void *, size_t);
char *strcpy(char *, const char *);
int snprintf(char *, size_t, const char *, ...);
int vsnprintf(char *, size_t, const char *, va_list);
int puts(const char *);

#ifndef __has_builtin
#define __has_builtin(x) 0
#endif

#define guard_obsz0(object) __builtin_object_size(object, 0)
#define guard_obsz(object) __builtin_object_size(object, 1)

#if __has_builtin(__builtin___memcpy_chk) || defined(__GNUC__)
#undef memcpy
#define memcpy(dest, ...) __builtin___memcpy_chk(dest, __VA_ARGS__, guard_obsz0(dest))
#endif

#if __has_builtin(__builtin___strcpy_chk) || defined(__GNUC__)
#undef strcpy
#define strcpy(dest, ...) __builtin___strcpy_chk(dest, __VA_ARGS__, guard_obsz(dest))
#endif

#if __has_builtin(__builtin___snprintf_chk) || defined(__GNUC__)
#undef snprintf
#define snprintf(str, len, ...) \
    __builtin___snprintf_chk(str, len, 0, guard_obsz(str), __VA_ARGS__)
#endif

#if __has_builtin(__builtin___vsnprintf_chk) || defined(__GNUC__)
#undef vsnprintf
#define vsnprintf(str, len, format, ap) \
    __builtin___vsnprintf_chk(str, len, 0, guard_obsz(str), format, ap)
#endif

#endif
