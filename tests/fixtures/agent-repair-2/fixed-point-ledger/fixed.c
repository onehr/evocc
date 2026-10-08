/* Fixed-point account processing. Amounts are integer cents.
 * Account deltas are in -100000000..100000000 and rates in -2000..2000.
 * A fee is abs(delta) * rate / 10000, rounded to nearest, ties away
 * from zero. Negative rates are rebates. Failed batches are atomic.
 */
typedef struct {
    long long balance;
    long long fees;
    unsigned accepted;
    unsigned rejected;
} Account;

typedef struct {
    unsigned account;
    int delta;
    int rate;
} Posting;

typedef struct {
    Account accounts[4];
    unsigned batches;
    unsigned rollbacks;
} Ledger;

static long long rounded_fee(int amount, int rate)
{
    long long product = (long long)amount * rate;
    if (product < 0)
        return (product - 5000) / 10000;
    return (product + 5000) / 10000;
}

static int magnitude(int value)
{
    if (value < 0)
        return -value;
    return value;
}

static void account_initialize(Account *account, long long balance)
{
    account->balance = balance;
    account->fees = 0;
    account->accepted = 0;
    account->rejected = 0;
}

static void initialize(Ledger *ledger, unsigned seed)
{
    unsigned i;
    for (i = 0; i < 4; ++i)
        account_initialize(&ledger->accounts[i], 200000000LL + seed * 101U + i * 997U);
    ledger->batches = 0;
    ledger->rollbacks = 0;
}

static int apply(Account *account, const Posting *posting)
{
    long long fee;
    long long balance;
    if (posting->delta < -100000000 || posting->delta > 100000000 ||
        posting->rate < -2000 || posting->rate > 2000) {
        ++account->rejected;
        return 0;
    }
    fee = rounded_fee(magnitude(posting->delta), posting->rate);
    balance = account->balance + posting->delta - fee;
    if (balance < 0 || balance > 1000000000LL) {
        ++account->rejected;
        return 0;
    }
    account->balance = balance;
    account->fees += fee;
    ++account->accepted;
    return 1;
}

static int batch(Ledger *ledger, const Posting *postings, unsigned count)
{
    Account checkpoint[4];
    unsigned i;
    unsigned failed = 0;
    for (i = 0; i < 4; ++i)
        checkpoint[i] = ledger->accounts[i];
    for (i = 0; i < count; ++i) {
        if (postings[i].account >= 4 ||
            !apply(&ledger->accounts[postings[i].account], &postings[i])) {
            failed = 1;
            break;
        }
    }
    ++ledger->batches;
    if (failed) {
        for (i = 0; i < 4; ++i)
            ledger->accounts[i] = checkpoint[i];
        ++ledger->rollbacks;
        return 0;
    }
    return 1;
}

static unsigned long long fingerprint(const Ledger *ledger)
{
    unsigned long long result = ledger->batches * 17U + ledger->rollbacks;
    unsigned i;
    for (i = 0; i < 4; ++i) {
        const Account *account = &ledger->accounts[i];
        result = result * 65599U + (unsigned long long)account->balance;
        result = result * 65599U + (unsigned long long)account->fees;
        result = result * 65599U + account->accepted;
        result = result * 65599U + account->rejected;
    }
    return result;
}

static void make_postings(Posting *postings, unsigned seed, unsigned block)
{
    unsigned i;
    for (i = 0; i < 6; ++i) {
        unsigned index = block * 6 + i;
        int amount = 200 + (int)((index * 719U + seed * 331U) % 15000U);
        postings[i].account = (index + seed) % 4;
        postings[i].delta = (index & 1U) ? -amount : amount;
        postings[i].rate = (index % 3U == 0) ? -37 : 113;
    }
    if (seed == 11 && block == 2) {
        postings[4].delta = 100000000;
        postings[4].rate = 311;
    }
}

/* Every unsigned seed is accepted; reduce it before constructing amounts. */
unsigned long long evaluate(unsigned seed)
{
    Ledger ledger;
    Posting postings[6];
    unsigned block;
    unsigned long long history = 0;
    seed %= 12;
    initialize(&ledger, seed);
    for (block = 0; block < 4; ++block) {
        make_postings(postings, seed, block);
        batch(&ledger, postings, 6);
        history = history * 257U + fingerprint(&ledger);
    }
    make_postings(postings, seed, 4);
    postings[5].account = 4;
    batch(&ledger, postings, 6);
    history ^= fingerprint(&ledger);
    postings[5].account = seed % 4;
    postings[5].delta = -100000000;
    postings[5].rate = -19;
    batch(&ledger, postings, 6);
    return history * 257U + fingerprint(&ledger);
}

int main(void)
{
    unsigned long long checksum = evaluate(0);
    checksum ^= evaluate(1);
    checksum += evaluate(2);
    checksum ^= evaluate(11);
    return checksum == 0xffffffffffffffffULL;
}
