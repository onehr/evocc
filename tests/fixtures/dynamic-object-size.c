typedef unsigned long size_t;
static unsigned char storage[256];
static unsigned char *volatile storage_pointer = storage;
static int calls;

__attribute__((noinline, alloc_size(1)))
static void *allocate(size_t size) {
    ++calls;
    return storage_pointer + (size & 1);
}

__attribute__((noinline, alloc_size(1, 2)))
static void *allocate_pair(size_t count, size_t size) {
    ++calls;
    return storage_pointer + ((count * size) & 1);
}

__attribute__((noinline))
static int capture(size_t size, int choice, size_t index) {
    size_t expected = size;
    char *p = allocate(size++);
    char *q = p + index++;
    char *r = choice ? allocate(9) : allocate(17);
    char *s = allocate_pair(3, 7);
    size = 99;
    choice = !choice;
    index = 88;
    if (__builtin_dynamic_object_size(p, 0) != expected) return 1;
    if (__builtin_dynamic_object_size(q, 1) != expected - 2) return 2;
    if (__builtin_dynamic_object_size(r, 1) != (choice ? 17 : 9)) return 3;
    if (__builtin_dynamic_object_size(s, 1) != 21) return 4;
    if (__builtin_object_size(p, 1) != (size_t)-1) return 5;
    return size != 99 || index != 88;
}

int main(void) {
    struct F { int head; char data[]; };
    union { struct F f; char backing[36]; } u = {0};
    struct F *p = (struct F *)&u;
    if (__builtin_dynamic_object_size(p, 0) != 36) return 10;
    if (__builtin_dynamic_object_size(p, 1) != 36) return 11;
    if (__builtin_dynamic_object_size(p->data, 1) != 32) return 12;
    if (__builtin_dynamic_object_size(p->data + 4, 0) != 28) return 13;
    char array[13];
    char *side = array;
    if (__builtin_dynamic_object_size(side++, 1) != (size_t)-1) return 14;
    if (__builtin_dynamic_object_size(side++, 3) != 0) return 15;
    if (side != array) return 16;
    int first = capture(13, 0, 2);
    if (first) return 30 + first;
    int second = capture(21, 1, 2);
    if (second) return 40 + second;
    if (calls != 6) return 19;
    char *constant = allocate(25);
    if (__builtin_object_size(constant, 1) != 25) return 20;
    return calls != 7;
}
