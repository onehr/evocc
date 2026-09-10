typedef unsigned __int128 u128;
typedef int (*callback)(void);
static unsigned int *vram = (unsigned int *)0xa000;
static void *negative = (void *)(signed char)-1;
static void *byte = (void *)(unsigned char)-1;
static void *word = (void *)(unsigned int)-1;
static void *truncated = (void *)(((u128)1 << 100) | 0x1234);
static void *roundtrip = (void *)(unsigned long)(char *)0xa000;
static callback absolute_function = (callback)0x2340;
static void *selected = 1 ? (void *)0x3450 : (void *)0x4560;
static void *unsigned_condition = (void *)(1 ? -1 : 1U);
struct addresses { int tag; void *pointer; };
static struct addresses entries[4] = {
    [0 ... 3] = {7, (void *)2}, [2] = {9, (void *)0x80}
};
static int object = 11;
static int *symbolic = &object;
static union { unsigned long bits; void *pointer; } encoded = {.bits = 0x5670};
static union { struct { unsigned long low:16; unsigned long high:48; } bits; void *pointer; } field_encoded = {.bits = {0x6780, 0}};

int main(void) {
    if ((unsigned long)vram != 0xa000) return 1;
    if ((unsigned long)negative != ~0UL) return 2;
    if ((unsigned long)byte != 255) return 3;
    if ((unsigned long)word != 0xffffffffUL) return 4;
    if ((unsigned long)truncated != 0x1234) return 5;
    if ((unsigned long)roundtrip != 0xa000) return 6;
    if ((unsigned long)absolute_function != 0x2340) return 7;
    if ((unsigned long)selected != 0x3450) return 8;
    struct addresses copy = entries[1];
    if (copy.tag != 7 || (unsigned long)copy.pointer != 2) return 9;
    if (entries[2].tag != 9 || (unsigned long)entries[2].pointer != 0x80) return 10;
    if (*symbolic != 11) return 11;
    if ((unsigned long)encoded.pointer != 0x5670) return 12;
    if ((unsigned long)field_encoded.pointer != 0x6780) return 13;
    if ((unsigned long)unsigned_condition != 0xffffffffUL) return 14;
    return 0;
}
