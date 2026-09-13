#if CASE == 1
#pragma GCC visibility push(hidden)
extern int data;
#pragma GCC visibility pop
extern int data;
#elif CASE == 2
extern int data;
#pragma GCC visibility push(hidden)
extern int data;
#elif CASE == 3
extern int data __attribute__((visibility("default")));
#pragma GCC visibility push(hidden)
extern int data;
#elif CASE == 4
#pragma GCC visibility push(hidden)
extern int data;
extern int data __attribute__((visibility("default")));
#elif CASE == 5
#pragma GCC visibility push(hidden)
extern int data __attribute__((visibility("default")));
#elif CASE == 6
#pragma GCC visibility pop
extern int data;
#elif CASE == 7
#pragma GCC visibility push(protected)
extern int data;
#elif CASE == 8
void f(void) {
#pragma GCC visibility push(hidden)
  extern int data;
}
extern int data;
#elif CASE == 9
void f(void) {
#pragma GCC visibility push(hidden)
  extern int data;
#pragma GCC visibility pop
}
extern int data;
#else
#if HIDDEN
#pragma GCC visibility push(hidden)
#endif
extern int data;
extern int weak_data __attribute__((weak));
int *address_data(void) { return &data; }
int *address_weak(void) { return &weak_data; }
#endif
int read_data(void) { return data; }
