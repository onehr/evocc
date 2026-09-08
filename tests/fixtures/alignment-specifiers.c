_Alignas(32) int raised;
unsigned short _Alignas(16) inputs[128];
long _Alignas(long) typed;
_Alignas(0) long unchanged;
_Alignas(1) _Alignas(16) int combined;
struct Record { char head; _Alignas(16) char member; };
_Alignas(16) struct Standalone { int member; };
_Static_assert(__alignof__(raised) == 32, "object alignment");
_Static_assert(__alignof__(__typeof__(raised)) == 4, "not type identity");
_Static_assert(__alignof__(inputs) == 16, "specifier order");
_Static_assert(__alignof__(typed) == 8, "type operand");
_Static_assert(__alignof__(unchanged) == 8, "zero ignored");
_Static_assert(__alignof__(combined) == 16, "maximum");
_Static_assert(sizeof(struct Record) == 32, "record size");
_Static_assert(_Alignof(struct Record) == 16, "record alignment");
_Static_assert(__builtin_offsetof(struct Record, member) == 16, "member offset");
_Static_assert(_Alignof(struct Standalone) == 4, "no object to align");
int main(void) {
    _Alignas(16) int local = 7, other = 11;
    _Static_assert(__alignof__(local) == 16, "automatic alignment");
    _Static_assert(__alignof__(other) == 16, "each declarator");
    for (int i = 0; i < 128; ++i) inputs[i] = i;
    unsigned short sum = 0;
    for (int i = 0; i < 128; i += 8) sum += inputs[i];
    if (((unsigned long)&raised & 31) || ((unsigned long)inputs & 15)) return 1;
    if (((unsigned long)&local & 15) || ((unsigned long)&other & 15)) return 2;
    return local != 7 || other != 11 || sum != 960;
}
