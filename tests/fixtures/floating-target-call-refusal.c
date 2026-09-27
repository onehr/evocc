/* Named refusals for target floating types are not executable support. */
#ifndef FLOAT_TYPE
#define FLOAT_TYPE __fp16
#endif
#ifndef CASE
#define CASE 0
#endif
typedef FLOAT_TYPE target_float;

#if CASE == 0
target_float identity(target_float value) { return value; }
#elif CASE == 1
int ignore(target_float value) { return 0; }
#elif CASE == 2
extern target_float external(void);
int main(void) { external(); return 0; }
#elif CASE == 3
int callback(target_float (*function)(target_float)) { return 0; }
#elif CASE == 4
typedef target_float signature(target_float);
signature *address(void) { return 0; }
#elif CASE == 5
/* Layout and pointers to storage do not execute the floating type. */
unsigned size(const target_float *pointer) { return sizeof(*pointer); }
int main(void) { return size(0) != sizeof(target_float); }
#else
#error unsupported CASE
#endif
