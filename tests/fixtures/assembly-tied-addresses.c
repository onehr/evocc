#define RELOC_HIDE(ptr, off) \
({ unsigned long __ptr; __asm__("" : "=r"(__ptr) : "0"(ptr)); \
   (typeof(ptr)) (__ptr + (off)); })

static int object[2] = {7, 11};
static int evaluations;
static int *get_object(void) { ++evaluations; return object; }
static int callback(void) { return 13; }

int main(void) {
    int *hidden = RELOC_HIDE(get_object(), sizeof(int));
    if (hidden != &object[1] || *hidden != 11 || evaluations != 1) return 1;
    unsigned long bits;
    __asm__("" : "=r"(bits) : "0"(&object[0]));
    void *restored;
    __asm__("" : "=r"(restored) : "0"(bits));
    if (restored != object) return 2;
    __asm__("" : "=r"(bits) : "0"(callback));
    int (*function)(void);
    __asm__("" : "=r"(function) : "0"(bits));
    if (function != callback || function() != 13) return 3;
    __asm__ goto("" : "=r"(restored) : "0"((unsigned long)object) : : done);
done:
    return restored != object;
}
