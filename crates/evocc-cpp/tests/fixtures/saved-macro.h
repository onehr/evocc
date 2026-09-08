#define VALUE(x) (x + 7)
#pragma push_macro("VALUE")
#undef VALUE
#define VALUE 99
