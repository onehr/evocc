#if CASE == 1
void *probe(void) { return __builtin_return_address(-1); }
#elif CASE == 2
void *probe(void) { return __builtin_frame_address(-1); }
#elif CASE == 3
void *probe(void) { return __builtin_return_address(0x100000001UL); }
#elif CASE == 4
void *probe(void) { return __builtin_frame_address(0x100000001UL); }
#elif CASE == 5
void *probe(unsigned n) { return __builtin_return_address(n); }
#elif CASE == 6
void *probe(unsigned n) { return __builtin_frame_address(n); }
#elif CASE == 7
void *probe(void) { return __builtin_frame_address(16); }
#elif CASE == 8
void *probe(void) { return __builtin_return_address(1); }
#elif CASE == 9
void *probe(void) { return __builtin_frame_address(1); }
#elif CASE == 10
void *probe(void) { return __builtin_frame_address(65536); }
#elif CASE == 11
void *probe(void) { return __builtin_return_address(0x100000000UL); }
#endif
