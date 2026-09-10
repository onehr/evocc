#if CASE == 1
void probe(void) { const long value[2] = {1, 2}; asm("" : "=m"(value)); }
#elif CASE == 2
struct record { const long value; };
void probe(void) { struct record value = {1}; asm("" : "=m"(value)); }
#elif CASE == 3
void probe(long value) { asm("" : : "m"(value + 1)); }
#elif CASE == 4
void probe(void) { long value[2] = {1, 2}; asm("" : "+m"(value)); }
#elif CASE == 5
void probe(long (*value)[2]) { asm("" : "+m"(*value)); }
#endif
