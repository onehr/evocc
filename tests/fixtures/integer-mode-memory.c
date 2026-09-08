typedef unsigned int byte __attribute__((mode(QI)));
typedef signed int signed_byte __attribute__((mode(QI)));
typedef unsigned int wide __attribute__((mode(DI)));
struct __attribute__((packed)) record {
    byte head;
    unsigned int value __attribute__((mode(DI)));
    byte tail;
};
static byte narrow(byte x) { return x; }
int main(void) {
    byte values[3] = {170, 170, 170};
    __builtin_memset(values + 1, 0, 1);
    struct record item = {7, 0, 9};
    item.value = 0x123456789abcdef0UL;
    return values[0] != 170 || values[1] != 0 || values[2] != 170
        || sizeof(item) != 10 || item.head != 7 || item.tail != 9
        || item.value != (wide)0x123456789abcdef0UL
        || narrow(511) != 255 || (signed_byte)255 != -1;
}
