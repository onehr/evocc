struct incomplete;
struct field { int is_signed; };
static struct field fields[] = {
    { .is_signed = (struct incomplete *)-1 < (struct incomplete *)1 },
    { .is_signed = (int *)0 < (int *)1 },
    { .is_signed = (int *)1 > (int *)0 },
    { .is_signed = (char *)-1 >= (char *)1 },
    { .is_signed = (void *)1 <= (void *)1 },
    { .is_signed = (void *)0 == (void *)0 },
    { .is_signed = (void *)1 != (void *)0 },
};
int main(void) {
    static int narrowed = (char *)(unsigned char)-1 < (char *)256;
    if (!narrowed || fields[0].is_signed) return 1;
    for (int i = 1; i < 7; ++i) if (!fields[i].is_signed) return 2;
    return 0;
}
