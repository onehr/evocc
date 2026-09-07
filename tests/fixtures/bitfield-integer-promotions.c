struct Fields {
    unsigned int u3 : 3;
    signed long s31 : 31;
    signed long s32 : 32;
    unsigned long u31 : 31;
    unsigned long u32 : 32;
    unsigned long long ull3 : 3;
};
struct Fields fields;

int main(void) {
    if (fields.u3 - 2 >= 0) return 1;
    if (fields.s31 - 2 >= 0) return 2;
    if (fields.s32 - 2 >= 0) return 3;
    if (fields.u31 - 2 >= 0) return 4;
    if (fields.u32 - 2 < 0) return 5;
    if (fields.ull3 - 2 >= 0) return 6;
    if (sizeof(+fields.s31) != 4) return 7;
    if (sizeof(+fields.s32) != 4) return 8;
    if (sizeof(+fields.u31) != 4) return 9;
    if (sizeof(+fields.u32) != 4) return 10;
    if (sizeof(+fields.ull3) != 4) return 11;
    return 0;
}
