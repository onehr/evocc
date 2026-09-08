#define slot state->slot
#define ID(x) x
#define WRAP(x) ID(x)
#define LEFT RIGHT
#define RIGHT LEFT
#define F(x) F(x)
#define NAME(x) x
#define CALL(x) NAME(x)
#define RAW(...) #__VA_ARGS__
#define STR(...) RAW(__VA_ARGS__)
#define CAT(a, b) a ## b
STR(ID(slot))
STR(WRAP(slot))
STR(ID(LEFT))
STR(ID(F(1)))
STR(CALL(ID)(slot))
STR(ID(slot) slot)
STR(CAT(sl, ot))
STR(ID(ID)(7))
STR(F(F(1)))
