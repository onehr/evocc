#ifndef CASE
#define CASE 0
#endif

int object = 7;
const int scalar = 13;
const int zero;
const int array[2][2] = { { 1, 2 }, { 3, 4 } };
int *const pointers[] = { &object };
const int *pointee = &scalar;
const volatile int device = 17;
struct partly_const { const int first; int second; } mixed = { 3, 5 };
int *literal = (const int[]){ 19, 23 };
int *mutable_literal = (int[]){ 29, 31 };
const int explicit_data __attribute__((section(".data.explicit"))) = 37;

int main(void) {
    static const int local = 41;
#if CASE == 1
    *(int *)&scalar = 1;
#elif CASE == 2
    ((int *)array)[3] = 1;
#elif CASE == 3
    ((int **)pointers)[0] = 0;
#elif CASE == 4
    literal[0] = 1;
#elif CASE == 5
    *(int *)&local = 1;
#elif CASE == 6
    *(int *)&zero = 1;
#elif CASE == 7
    *(int *)&explicit_data = 1;
#else
    object = 11;
    pointee = &object;
    mixed.second = 7;
    mutable_literal[0] = 43;
    return scalar != 13 || zero != 0 || array[1][1] != 4 ||
           *pointers[0] != 11 || *pointee != 11 || device != 17 ||
           mixed.second != 7 || literal[1] != 23 || local != 41 ||
           mutable_literal[0] != 43 || explicit_data != 37;
#endif
    return 0;
}
