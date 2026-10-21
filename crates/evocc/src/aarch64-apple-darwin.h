/* The predefined macros of arm64 macOS that differ from AArch64 Linux.
 * The driver applies this file on top of the Linux profile of evocc-cpp.
 * The values come from Apple clang for arm64-apple-macos. The compiler
 * identity stays the one of the Linux profile, so the headers take their
 * GCC paths. */

/* The operating system and the object format. */
#undef __linux
#undef __linux__
#undef __gnu_linux__
#undef linux
#undef __unix
#undef __unix__
#undef unix
#undef __ELF__
#undef _STDC_PREDEF_H
#undef __STDC_ISO_10646__
#define __APPLE__ 1
#define __APPLE_CC__ 6000
#define __MACH__ 1
#define __arm64 1
#define __arm64__ 1
#define __ARM64_ARCH_8__ 1
#define __AARCH64_SIMD__ 1
#define __ARM_NEON__ 1
#define __LITTLE_ENDIAN__ 1
#define __DYNAMIC__ 1
#define __STDC_NO_THREADS__ 1
#undef __USER_LABEL_PREFIX__
#define __USER_LABEL_PREFIX__ _
#undef __BIGGEST_ALIGNMENT__
#define __BIGGEST_ALIGNMENT__ 8

/* The 64-bit integer types are long long. */
#undef __INT64_TYPE__
#define __INT64_TYPE__ long long int
#undef __INT64_MAX__
#define __INT64_MAX__ 0x7fffffffffffffffLL
#undef __INT64_C
#define __INT64_C(c) c ## LL
#undef __UINT64_TYPE__
#define __UINT64_TYPE__ long long unsigned int
#undef __UINT64_MAX__
#define __UINT64_MAX__ 0xffffffffffffffffULL
#undef __UINT64_C
#define __UINT64_C(c) c ## ULL
#undef __INT_LEAST64_TYPE__
#define __INT_LEAST64_TYPE__ long long int
#undef __INT_LEAST64_MAX__
#define __INT_LEAST64_MAX__ 0x7fffffffffffffffLL
#undef __UINT_LEAST64_TYPE__
#define __UINT_LEAST64_TYPE__ long long unsigned int
#undef __UINT_LEAST64_MAX__
#define __UINT_LEAST64_MAX__ 0xffffffffffffffffULL

/* The fast integer types have the width that they name. */
#undef __INT_FAST16_TYPE__
#define __INT_FAST16_TYPE__ short int
#undef __INT_FAST16_MAX__
#define __INT_FAST16_MAX__ 0x7fff
#undef __INT_FAST16_WIDTH__
#define __INT_FAST16_WIDTH__ 16
#undef __UINT_FAST16_TYPE__
#define __UINT_FAST16_TYPE__ short unsigned int
#undef __UINT_FAST16_MAX__
#define __UINT_FAST16_MAX__ 0xffff
#undef __INT_FAST32_TYPE__
#define __INT_FAST32_TYPE__ int
#undef __INT_FAST32_MAX__
#define __INT_FAST32_MAX__ 0x7fffffff
#undef __INT_FAST32_WIDTH__
#define __INT_FAST32_WIDTH__ 32
#undef __UINT_FAST32_TYPE__
#define __UINT_FAST32_TYPE__ unsigned int
#undef __UINT_FAST32_MAX__
#define __UINT_FAST32_MAX__ 0xffffffffU
#undef __INT_FAST64_TYPE__
#define __INT_FAST64_TYPE__ long long int
#undef __INT_FAST64_MAX__
#define __INT_FAST64_MAX__ 0x7fffffffffffffffLL
#undef __UINT_FAST64_TYPE__
#define __UINT_FAST64_TYPE__ long long unsigned int
#undef __UINT_FAST64_MAX__
#define __UINT_FAST64_MAX__ 0xffffffffffffffffULL

/* wint_t is int, and Apple clang defines no __WINT_MIN__. The driver sets
 * the wchar_t macros, because -fshort-wchar changes them. */
#undef __WINT_TYPE__
#define __WINT_TYPE__ int
#undef __WINT_MAX__
#define __WINT_MAX__ 2147483647
#undef __WINT_MIN__

/* long double is binary64. */
#undef __SIZEOF_LONG_DOUBLE__
#define __SIZEOF_LONG_DOUBLE__ 8
#undef __LDBL_DECIMAL_DIG__
#define __LDBL_DECIMAL_DIG__ 17
#undef __LDBL_DENORM_MIN__
#define __LDBL_DENORM_MIN__ 4.9406564584124654e-324L
#undef __LDBL_DIG__
#define __LDBL_DIG__ 15
#undef __LDBL_EPSILON__
#define __LDBL_EPSILON__ 2.2204460492503131e-16L
#undef __LDBL_MANT_DIG__
#define __LDBL_MANT_DIG__ 53
#undef __LDBL_MAX_10_EXP__
#define __LDBL_MAX_10_EXP__ 308
#undef __LDBL_MAX_EXP__
#define __LDBL_MAX_EXP__ 1024
#undef __LDBL_MAX__
#define __LDBL_MAX__ 1.7976931348623157e+308L
#undef __LDBL_MIN_10_EXP__
#define __LDBL_MIN_10_EXP__ (-307)
#undef __LDBL_MIN_EXP__
#define __LDBL_MIN_EXP__ (-1021)
#undef __LDBL_MIN__
#define __LDBL_MIN__ 2.2250738585072014e-308L
#undef __LDBL_NORM_MAX__
#define __LDBL_NORM_MAX__ 1.7976931348623157e+308L
#undef __DECIMAL_DIG__
#define __DECIMAL_DIG__ __LDBL_DECIMAL_DIG__

/* Apple clang has no bfloat16 constant suffix and predefines no bfloat16
 * macros (EvoIR 3.13.6). */
#undef __BFLT16_DECIMAL_DIG__
#undef __BFLT16_DENORM_MIN__
#undef __BFLT16_DIG__
#undef __BFLT16_EPSILON__
#undef __BFLT16_HAS_DENORM__
#undef __BFLT16_HAS_INFINITY__
#undef __BFLT16_HAS_QUIET_NAN__
#undef __BFLT16_IS_IEC_60559__
#undef __BFLT16_MANT_DIG__
#undef __BFLT16_MAX_10_EXP__
#undef __BFLT16_MAX_EXP__
#undef __BFLT16_MAX__
#undef __BFLT16_MIN_10_EXP__
#undef __BFLT16_MIN_EXP__
#undef __BFLT16_MIN__
#undef __BFLT16_NORM_MAX__
