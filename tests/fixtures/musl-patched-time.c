/* Explicit dependency compatibility extension, not a compiler dialect change. */
#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static int checks, failures;
static void check(int valid, const char *label)
{
    ++checks;
    if (!valid) { ++failures; printf("failed: %s\n", label); }
}

static int same(const struct tm *a, const struct tm *b)
{
    return a->tm_year == b->tm_year && a->tm_mon == b->tm_mon &&
           a->tm_mday == b->tm_mday && a->tm_hour == b->tm_hour &&
           a->tm_min == b->tm_min && a->tm_sec == b->tm_sec &&
           a->tm_wday == b->tm_wday && a->tm_yday == b->tm_yday &&
           a->tm_isdst == b->tm_isdst && a->tm_gmtoff == b->tm_gmtoff;
}

int main(void)
{
    const time_t values[] = {-1, 0, 683078400, 2147483648LL};
    const char *zones[] = {"UTC0", "UTC5", "UTC-9"};
    for (int zone = 0; zone < 3; ++zone) {
        if (setenv("TZ", zones[zone], 1)) return 2;
        tzset();
        for (int i = 0; i < 4; ++i) {
            char text[64];
            snprintf(text, sizeof text, "%lld!", (long long)values[i]);
            struct tm tm = {0}, want;
            if (!localtime_r(&values[i], &want)) return 3;
            errno = EDOM;
            char *end = strptime(text, "%s", &tm);
            check(end && !strcmp(end, "!") && same(&tm, &want) && errno == EDOM, text);
        }
    }
    if (setenv("TZ", "UTC0", 1)) return 4;
    tzset();
    struct tm tm = {0};
    char *end = strptime("0", "%s", &tm);
    check(end && !*end && tm.tm_year == 70 && tm.tm_mon == 0 && tm.tm_mday == 1
          && tm.tm_wday == 4 && tm.tm_yday == 0 && tm.tm_hour == 0, "epoch fields");
    end = strptime("19 0", "%C %s", &tm);
    check(end && !*end && tm.tm_year == 70, "epoch replaces century");
    end = strptime("69 0", "%y %s", &tm);
    check(end && !*end && tm.tm_year == 70, "epoch replaces short year");
    end = strptime("0 72", "%s %y", &tm);
    check(end && !*end && tm.tm_year == 72, "subsequent year replaces epoch year");
    const char *bad_seconds[] = {"", "-", "+", "x", "9223372036854775808",
        "-9223372036854775809", "99999999999999999999999999999999999",
        "9223372036854775807", "-9223372036854775808"};
    for (int i = 0; i < 9; ++i) {
        struct tm before = tm;
        check(!strptime(bad_seconds[i], "%s", &tm) && same(&tm, &before), bad_seconds[i]);
    }
    const struct { const char *text; long offset; } offsets[] = {
        {"+00", 0}, {"-00", 0}, {"+02", 7200}, {"-06", -21600},
        {"+0200", 7200}, {"-0530", -19800}, {"+23", 82800},
        {"-2359", -86340}, {"+0001", 60}, {"-0001", -60}
    };
    for (int i = 0; i < 10; ++i) {
        end = strptime(offsets[i].text, "%z", &tm);
        check(end && !*end && tm.tm_gmtoff == offsets[i].offset && tm.tm_year == 72,
              offsets[i].text);
    }
    const char *bad_offsets[] = {"", "+", "-0", "+0x", "+010", "+2400", "-2460", "+0060"};
    for (int i = 0; i < 8; ++i) {
        long before = tm.tm_gmtoff;
        check(!strptime(bad_offsets[i], "%z", &tm) && tm.tm_gmtoff == before, bad_offsets[i]);
    }
    end = strptime("+02tail", "%z", &tm);
    check(end && !strcmp(end, "tail") && tm.tm_gmtoff == 7200, "hour-only suffix");
    end = strptime("-0530tail", "%z", &tm);
    check(end && !strcmp(end, "tail") && tm.tm_gmtoff == -19800, "hour-minute suffix");
    printf("checks=%d failures=%d\n", checks, failures);
    return failures != 0;
}
