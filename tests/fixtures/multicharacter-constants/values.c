/* Multi-character constants (EvoIR 10.5). GCC 14 and Apple clang 21 print the
 * same lines for this program; main returns 0 when every value is the one
 * that both compilers give. */
#include <stdio.h>
#if 'ab' == 0x6162 && 'TEXT' == 0x54455854 && '\xff\xff\xff\xff' < 0 && 'abcde' == 'bcde'
#define PP_OK 1
#else
#define PP_OK 0
#endif
int main(void)
{
    printf("%d %d %d %d\n", PP_OK, 'ab', 'abc', 'abcd');
    printf("%d %d %d\n", '\xff\xff', '\x0\xff', '\xff\xff\xff\xff');
    printf("%d %d %d\n", '\1\2', '\377\1', 'abcde');
    printf("%zu %d\n", sizeof('ab'), '\'\'');
    return !(PP_OK && 'ab' == 24930 && 'abc' == 6382179 && 'abcd' == 1633837924
             && '\xff\xff' == 65535 && '\x0\xff' == 255 && '\xff\xff\xff\xff' == -1
             && '\1\2' == 258 && '\377\1' == 65281 && 'abcde' == 1650680933
             && sizeof('ab') == sizeof(int) && '\'\'' == 10023);
}
