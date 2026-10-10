/* Find the first bad version in 1..n with a binary search.
   Versions from 1,700,000,000 on are bad. */

static int is_bad(int version) { return version >= 1700000000; }

int first_bad(int n) {
    int lo = 1, hi = n;
    while (lo < hi) {
        int mid = (lo + hi) / 2;
        if (is_bad(mid)) hi = mid;
        else lo = mid + 1;
    }
    return lo;
}

int main(void) { return first_bad(2000000000) == 1700000000 ? 0 : 1; }
