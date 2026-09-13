/* Oracle probe for the pending object-size profile decision; not a subject PASS. */
#include <stdio.h>

struct volume { char prefix[7]; char name[16]; char tail[5]; };

static void member(struct volume *v)
{
    printf("member=%lu/%lu/%lu/%lu offset=%lu\n",
           __builtin_object_size(v->name, 0),
           __builtin_object_size(v->name, 1),
           __builtin_object_size(v->name, 2),
           __builtin_object_size(v->name, 3),
           __builtin_object_size(v->name + 3, 1));
}

int main(void)
{
    struct volume v;
    char a[16];
    char *p = a;
    int effects = 0;
    member(&v);
    unsigned long side = __builtin_object_size((effects++, p), 1);
    printf("array=%lu offset=%lu literal=%lu unknown=%lu side=%lu effects=%d\n",
           __builtin_object_size(a, 1),
           __builtin_object_size(a + 3, 1),
           __builtin_object_size("abc", 1),
           __builtin_object_size((void *)37, 1), side, effects);
    return 0;
}
