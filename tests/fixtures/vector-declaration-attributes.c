/* Direct declaration attributes must agree with typedef-based vector types. */
#define VECTOR long __attribute__((vector_size(16)))
typedef long lanes __attribute__((vector_size(16)));
static VECTOR global = {1, 2};

static long sum(VECTOR value) { return value[0] + value[1]; }
static VECTOR increment(VECTOR value) { return value + 1; }
static long array_sum(VECTOR values[2]) { return sum(values[0]) + sum(values[1]); }
struct record { VECTOR a, b[2]; };

int main(void) {
    VECTOR a = {3, 4}, b = {5, 6};
    long c __attribute__((vector_size(16))) = {7, 8};
    VECTOR d;
    d = a + b + c + global;
    if (!__builtin_types_compatible_p(__typeof__(a), lanes)) return 1;
    if (sizeof d != 16 || d[0] != 16 || d[1] != 20) return 2;
    if (sum(d) != 36) return 3;
    VECTOR array[2] = {{9, 10}, {11, 12}};
    VECTOR *pointer = &array[1];
    lanes control = increment(*pointer);
    if (sizeof array != 32 || control[0] != 12 || control[1] != 13) return 4;
    struct record record = {a, {{1, 2}, {3, 4}}};
    if (sizeof record != 48 || array_sum(record.b) != 10 || sum(record.a) != 7) return 5;
    return 0;
}
