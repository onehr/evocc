/* Frozen OVPN selftest dependency boundary, not a replacement for that test.
 * Its 256-byte read cuts the supplied 349-byte base64 file mid-quantum.
 * Exercise the exact runtime Mbed TLS library without any kernel/netlink call. */
#include <stddef.h>
#include <stdio.h>
#include <string.h>

extern int mbedtls_base64_decode(unsigned char *, size_t, size_t *,
                               const unsigned char *, size_t);

static unsigned count, failed;

static void check(int ok, const char *name)
{
    printf("%s %u %s\n", ok ? "ok" : "not ok", ++count, name);
    failed += !ok;
}

int main(int argc, char **argv)
{
    unsigned char input[1024], full[1024], prefix[1024];
    size_t needed = 0, full_length = 0, prefix_length = 0;
    FILE *file = argc == 2 ? fopen(argv[1], "rb") : NULL;
    if (!file)
        return 2;
    size_t length = fread(input, 1, sizeof(input), file);
    int valid = !ferror(file) && feof(file) && fclose(file) == 0;
    printf("TAP version 13\n1..7\n");
    check(valid && length == 349, "frozen complete input is 349 bytes");
    size_t symbols = 0;
    for (size_t i = 0; i < 256 && i < length; ++i)
        symbols += input[i] != '\n' && input[i] != '\r';
    check(symbols == 253 && symbols % 4 == 1,
          "original byte cap leaves an incomplete base64 quantum");
    int rc = mbedtls_base64_decode(NULL, 0, &needed, input, 256);
    printf("# capped query rc=%d\n", rc);
    check(rc == -0x2c, "original prefix rejected as invalid encoding");
    rc = mbedtls_base64_decode(NULL, 0, &needed, input, length);
    check(rc == -0x2a && needed == 256,
          "complete input requests 256 decoded bytes");
    rc = mbedtls_base64_decode(full, sizeof(full), &full_length, input, length);
    check(rc == 0 && full_length == 256, "complete original data decodes");
    rc = mbedtls_base64_decode(prefix, sizeof(prefix), &prefix_length, input, 255);
    check(rc == 0 && prefix_length == 189
          && memcmp(prefix, full, prefix_length) == 0,
          "one-byte-shorter aligned diagnostic prefix decodes identically");
    rc = mbedtls_base64_decode(prefix, sizeof(prefix), &prefix_length,
                               (const unsigned char *)"A", 1);
    check(rc == -0x2c, "one-symbol reduction independently reproduces rejection");
    return failed != 0 || count != 7;
}
