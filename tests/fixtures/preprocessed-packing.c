#define PACK(x) _Pragma(#x)
PACK(pack(push, 1))
struct first { char c; int i; };
#pragma pack(push, 2)
struct second { char c; int i; };
#pragma pack(pop)
struct restored { char c; int i; };
#pragma pack(pop)
struct normal { char c; int i; };
#pragma pack(1)
struct set { char c; int i; };
#pragma pack()
struct reset { char c; int i; };
#pragma pack(2)
#pragma pack(push)
#pragma pack(1)
struct bare_push { char c; int i; };
#pragma pack(pop)
struct bare_restore { char c; int i; };
#pragma pack()
_Static_assert(sizeof(struct first) == 5, "pack via macro");
_Static_assert(sizeof(struct second) == 6, "nested pack");
_Static_assert(sizeof(struct restored) == 5, "restore pack");
_Static_assert(sizeof(struct normal) == 8, "pop to default");
_Static_assert(sizeof(struct set) == 5, "set pack");
_Static_assert(sizeof(struct reset) == 8, "reset pack");
_Static_assert(sizeof(struct bare_push) == 5, "push current then set");
_Static_assert(sizeof(struct bare_restore) == 6, "restore saved current");
int main(void) { return 0; }
