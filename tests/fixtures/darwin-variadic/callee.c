#include <stdio.h>
#include "shapes.h"

double v_doubles(int n, ...)
{
    va_list ap;
    va_start(ap, n);
    double sum = 0;
    for (int i = 0; i < n; i++)
        sum += va_arg(ap, double) * (i + 1);
    va_end(ap);
    return sum;
}

/* int, long, double, pointer, char (as int) and float (as double). */
long v_mixed(int n, ...)
{
    va_list ap;
    va_start(ap, n);
    long sum = 0;
    for (int i = 0; i < n; i++) {
        sum += va_arg(ap, int);
        sum += va_arg(ap, long) * 3;
        sum += (long)(va_arg(ap, double) * 5);
        sum += *va_arg(ap, long *) * 7;
        sum += (signed char)va_arg(ap, int) * 11;
        sum += (long)(va_arg(ap, double) * 13);
    }
    va_end(ap);
    return sum;
}

long v_int128(int n, ...)
{
    va_list ap;
    va_start(ap, n);
    long sum = 0;
    for (int i = 0; i < n; i++) {
        sum += va_arg(ap, int);
        __int128 w = va_arg(ap, __int128);
        sum += (long)w * 3 + (long)(w >> 64) * 5;
    }
    va_end(ap);
    return sum;
}

long double v_long_double(int n, ...)
{
    va_list ap;
    va_start(ap, n);
    long double sum = 0;
    for (int i = 0; i < n; i++) {
        sum += va_arg(ap, int);
        sum += va_arg(ap, long double) * 3;
    }
    va_end(ap);
    return sum;
}

double v_hfa(int n, ...)
{
    va_list ap;
    va_start(ap, n);
    double sum = 0;
    for (int i = 0; i < n; i++) {
        struct h3 h = va_arg(ap, struct h3);
        struct h4d d = va_arg(ap, struct h4d);
        sum += h.a + h.b * 3 + h.c * 5 + d.a * 7 + d.d * 11;
    }
    va_end(ap);
    return sum;
}

long v_hva(int n, ...)
{
    va_list ap;
    va_start(ap, n);
    long sum = 0;
    for (int i = 0; i < n; i++) {
        struct hv2 h = va_arg(ap, struct hv2);
        struct hv1 g = va_arg(ap, struct hv1);
        v4i v = va_arg(ap, v4i);
        sum += h.a[0] + h.b[3] * 3 + g.a[1] * 5 + v[2] * 7;
    }
    va_end(ap);
    return sum;
}

long v_records(int n, ...)
{
    va_list ap;
    va_start(ap, n);
    long sum = 0;
    for (int i = 0; i < n; i++) {
        struct r3 r = va_arg(ap, struct r3);
        struct r12 q = va_arg(ap, struct r12);
        struct r16 s = va_arg(ap, struct r16);
        struct a16 a = va_arg(ap, struct a16);
        struct big b = va_arg(ap, struct big);
        sum += r.c[2] + q.c * 3 + s.b * 5 + a.a * 7 + a.b * 11 + b.a[4] * 13;
    }
    va_end(ap);
    return sum;
}

long v_copy(int n, ...)
{
    va_list ap, copy;
    va_start(ap, n);
    va_copy(copy, ap);
    long first = 0, second = 0;
    for (int i = 0; i < n; i++)
        first += va_arg(ap, long) * (i + 1);
    for (int i = 0; i < n; i++)
        second += va_arg(copy, long);
    va_end(copy);
    va_end(ap);
    return first * 1000 + second;
}

int v_format(char *buffer, size_t size, const char *format, ...)
{
    va_list ap;
    va_start(ap, format);
    int length = vsnprintf(buffer, size, format, ap);
    va_end(ap);
    return length;
}

long v_after_stack(long a0, long a1, long a2, long a3, long a4, long a5, long a6, long a7,
                   char c, int i, ...)
{
    va_list ap;
    va_start(ap, i);
    double d = va_arg(ap, double);
    long l = va_arg(ap, long);
    int k = va_arg(ap, int);
    va_end(ap);
    return a0 + a7 + c * 3 + i * 5 + (long)(d * 7) + l * 11 + k * 13;
}

long sum_list(int n, va_list list)
{
    long sum = 0;
    for (int i = 0; i < n; i++)
        sum += va_arg(list, long) * (i + 1);
    return sum;
}

long forward(int n, ...)
{
    va_list ap;
    va_start(ap, n);
    long sum = sum_list(n, ap);
    va_end(ap);
    return sum;
}

/* The named stack area uses SIMD words; the anonymous long uses the other bank. */
double v_after_vector_stack(double a, double b, double c, double d, double e,
                            double f, double g, double h, double i, ...)
{
    va_list ap;
    va_start(ap, i);
    long n = va_arg(ap, long);
    va_end(ap);
    return a + b + c + d + e + f + g + h + i + n;
}
