struct Record {
    int head;
    union { struct { int left, right; }; long raw; };
    int tail;
};
static struct Record global = { tail: 9, left: 5, head: 1, right: 7 };
static int sparse[] = { [3] 11, [1] 4, [5 ... 7] 13 };
_Static_assert(sizeof(sparse) == 32, "inferred bound");
int main(void) {
    int effects = 0;
    struct Record local = { head: ++effects, left: 2, right: 3, tail: 4 };
    if (effects != 1) return 1;
    if (local.head != 1 || local.left != 2 || local.right != 3 || local.tail != 4) return 2;
    if (global.head != 1 || global.left != 5 || global.right != 7 || global.tail != 9) return 3;
    if (sparse[0] || sparse[1] != 4 || sparse[2] || sparse[3] != 11 || sparse[4]) return 4;
    if (sparse[5] != 13 || sparse[6] != 13 || sparse[7] != 13) return 5;
    local = (struct Record){ right: effects++, tail: 8 };
    if (local.head || local.left || local.right != 1 || local.tail != 8 || effects != 2) return 6;
    return 0;
}
