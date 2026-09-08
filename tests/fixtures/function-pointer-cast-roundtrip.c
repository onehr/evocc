static int calls;
static int identity(int value) { ++calls; return value + 1; }
static void other(void) { calls += 100; }
typedef int (*original)(int);
typedef long (*different)(long, long);
static different stored;
int main(void) {
    original first = identity;
    stored = (different)first;
    original restored = (original)stored;
    if (restored(6) != 7 || calls != 1) return 1;
    ((int (*)(int))other)(({ ++calls; goto done; 5; }));
    return 2;
done:
    return calls != 2;
}
