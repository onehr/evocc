struct flags { volatile unsigned low:3; unsigned keep:5; volatile signed value:8; };
struct wide { volatile unsigned long field:9; unsigned long keep:55; };
struct overlap { unsigned char prefix; volatile unsigned field:9; };
struct all_volatile { unsigned first:7; unsigned second:7; };
struct __attribute__((packed)) packed_fields { volatile unsigned a:9; volatile unsigned b:7; };
struct __attribute__((packed)) packed_three { volatile unsigned a:19; unsigned b:5; };

static struct flags small = {2, 17, -3};
static struct wide large = {511, 123456};
static struct overlap overlapping = {37, 400};
static volatile struct all_volatile whole = {100, 73};
static struct packed_fields packed = {511, 73};
static struct packed_three three = {123456, 17};
static int calls;
static struct flags *get(void) { calls++; return &small; }

unsigned read_small(struct flags *p) { return p->low; }
void write_small(struct flags *p, unsigned v) { p->low = v; }
int increment_small(struct flags *p) { return p->value++; }
unsigned long read_large(struct wide *p) { return p->field; }
void write_large(struct wide *p, unsigned long v) { p->field = v; }
unsigned read_three(struct packed_three *p) { return p->a; }
void write_three(struct packed_three *p, unsigned v) { p->a = v; }

int main(void) {
    if (sizeof(struct flags) != 4 || sizeof(struct wide) != 8) return 1;
    if (read_small(&small) != 2 || read_large(&large) != 511) return 2;
    write_small(&small, 5);
    if (small.low != 5 || small.keep != 17 || small.value != -3) return 3;
    if (increment_small(&small) != -3 || small.value != -2) return 4;
    write_large(&large, 0);
    if (large.field != 0 || large.keep != 123456) return 5;
    get()->low += 2;
    if (calls != 1 || small.low != 7 || small.keep != 17) return 6;
    if (++get()->low != 0 || calls != 2 || small.keep != 17) return 7;
    overlapping.field = 13;
    if (overlapping.prefix != 37 || overlapping.field != 13) return 8;
    whole.first -= 1;
    if (whole.first != 99 || whole.second != 73) return 9;
    packed.a = 2;
    if (sizeof(packed) != 2 || packed.a != 2 || packed.b != 73) return 10;
    if (sizeof(three) != 3 || read_three(&three) != 123456) return 11;
    write_three(&three, 70000);
    if (three.a != 70000 || three.b != 17) return 12;
    return 0;
}
