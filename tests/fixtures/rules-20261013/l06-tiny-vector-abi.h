/* L06 probe interface: GNU integer vectors below 8 bytes at a call boundary. */
typedef unsigned char v1 __attribute__((vector_size(1)));
typedef unsigned char v2 __attribute__((vector_size(2)));
typedef unsigned char v4 __attribute__((vector_size(4)));
typedef unsigned short s2 __attribute__((vector_size(4)));
struct holder { v4 x; int tag; };

extern volatile unsigned seen;
void take4(int lead, v4 x, v2 y, v1 z);   /* arguments only */
v4 give4(void);                           /* result only */
v2 give2(void);
s2 give_s2(void);
v4 echo4(v4 x);                           /* both directions */
struct holder echo_holder(struct holder h);
unsigned sum_variadic(int count, ...);    /* v4 through varargs */
