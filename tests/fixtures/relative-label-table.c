static int total;
__attribute__((noinline))
static void dispatch(int index) {
    static const int offsets[] = { &&first - &&end, &&second - &&end };
    goto *(&&end + offsets[index]);
first:
    total += 2;
second:
    ++total;
end:;
}
int main(void) {
    dispatch(0);
    if (total != 3) return 1;
    dispatch(1);
    return total != 4;
}
