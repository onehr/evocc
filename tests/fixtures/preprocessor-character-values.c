#if L'\400' != 256 || '\101' != 65 || '\x41' != 'A' || '\e' != 27
#error bad_numeric_escape
#endif
#if L'\u0100' != 256 || '\v' != 11 || '\a' != 7 || '\b' != 8 || '\f' != 12
#error bad_character_value
#endif
#define C L'\400'
#if C
#define zero (!C)
#else
#define zero C
#endif
int main(void) { return zero != 0; }
