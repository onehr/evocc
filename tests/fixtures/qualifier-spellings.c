__volatile int value = 1;
__const int fixed = 3;
__const__ int other = 7;
__signed int negative = -1;
struct record { __const int *pointer; };

int main(void) {
    struct record r = { &fixed };
    __volatile int * __const pointer = &value;
    *pointer = 5;
    return value != 5 || *r.pointer != 3 || other != 7 || negative >= 0;
}
