typedef union {
    __extension__ unsigned long long int value64;
    struct { unsigned int low, high; } value32;
} counter_t;

struct record {
    __extension__ __extension__ unsigned int low : 3;
    __extension__ struct { int value; };
    __extension__ int left, right;
};

int main(void) {
    counter_t counter = { .value64 = 42 };
    struct record r = { .low = 5, .value = 7, .left = 2, .right = 3 };
    return sizeof(counter) != 8 || counter.value64 != 42 ||
           r.low != 5 || r.value != 7 || r.left + r.right != 5;
}
