typedef int i4 __attribute__((vector_size(16)));
typedef short s4 __attribute__((vector_size(8)));
typedef int i8 __attribute__((vector_size(32)));
struct hva { i4 x, y, z; };
i4 vector_add(i4, i4);
s4 vector_small(s4, s4);
long vector_ninth(int, i4, i4, i4, i4, i4, i4, i4, i4, i4, long);
struct hva vector_hva(struct hva, i4);
i8 vector_wide(i8);
i4 vector_preserved(i4);
int vector_callback(int);
long vector_mixed_stack(long,long,long,long,long,long,long,long,
                       s4,s4,s4,s4,s4,s4,s4,s4,s4,__int128);
struct hva vector_hva_stack(i4,i4,i4,i4,i4,i4,struct hva,long);
