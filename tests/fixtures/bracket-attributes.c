[[gnu::noinline, gnu::unused]] static int value(void) { return 7; }
[[gnu::aligned(32)]] __attribute__((unused)) static int object;
struct [[gnu::packed]] Packet {
    char a;
    [[gnu::aligned(2)]] int b;
};
[[aligned(32)]] int plain;
[[other::aligned(32)]] int foreign;
[[__gnu__::__aligned__(32)]] int underscored;
struct [[packed]] Plain { char a; int b; };
typedef int number;
int main(void) {
    [[gnu::unused]] number local = value();
    struct Packet p = { 1, 3 };
    return local != 7 || __alignof__(object) != 32
        || sizeof p != 6 || __alignof__(struct Packet) != 2
        || __builtin_offsetof(struct Packet, b) != 2 || p.b != 3
        || __alignof__(plain) != 4 || __alignof__(foreign) != 4
        || __alignof__(underscored) != 32 || sizeof(struct Plain) != 8;
}
