struct Small { int x; };
struct Pair { long x, y; };
struct Large { long x, y, z; };
long aggregate_sum(int, ...);
long aggregate_stacked(long, long, long, long, long, long, long, ...);
int main(void) {
    struct Small a = {2};
    struct Pair b = {3, 5};
    struct Large c = {7, 11, 13};
    long (*indirect)(int, ...) = aggregate_sum;
    return aggregate_sum(1, a, b, c, 17L) != 61 ||
           indirect(1, a, b, c, 17L) != 61 ||
           aggregate_stacked(1, 2, 3, 4, 5, 6, 7, b, c, 17L) != 84;
}
