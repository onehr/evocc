/* The values of values.c without the C library, for the interpreters. */
#if 'ab' == 0x6162 && 'TEXT' == 0x54455854 && '\xff\xff\xff\xff' < 0 && 'abcde' == 'bcde'
#define PP_OK 1
#else
#define PP_OK 0
#endif
int main(void)
{
    return !(PP_OK && 'ab' == 24930 && 'abc' == 6382179 && 'abcd' == 1633837924
             && '\xff\xff' == 65535 && '\x0\xff' == 255 && '\xff\xff\xff\xff' == -1
             && '\1\2' == 258 && '\377\1' == 65281 && 'abcde' == 1650680933
             && sizeof('ab') == sizeof(int) && '\'\'' == 10023);
}
