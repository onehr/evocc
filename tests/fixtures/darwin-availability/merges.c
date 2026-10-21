/* Merged availability attributes, as Apple clang 21 merges them: an
 * attribute that disagrees with the merged one removes both, and agreeing
 * attributes fill in each other's versions. */
int two_a(void) __attribute__((availability(macos,introduced=13.0))) __attribute__((availability(macos,introduced=10.0)));
int two_b(void) __attribute__((availability(macos,introduced=10.0))) __attribute__((availability(macos,introduced=13.0)));
int two_c(void) __attribute__((availability(macos,introduced=13.0))) __attribute__((availability(macos,introduced=14.0)));
int re_a(void) __attribute__((availability(macos,introduced=13.0)));
int re_a(void) __attribute__((availability(macos,introduced=10.0)));
int re_c(void) __attribute__((availability(macos,introduced=13.0)));
int re_c(void) __attribute__((availability(macos,introduced=14.0)));
int same(void) __attribute__((availability(macos,introduced=13.0))) __attribute__((availability(macos,introduced=13.0)));
int u_then_i(void) __attribute__((availability(macos,unavailable))) __attribute__((availability(macos,introduced=13.0)));
int dep_then_intro(void) __attribute__((availability(macos,deprecated=12.0))) __attribute__((availability(macos,introduced=13.0)));
int mixed(void) __attribute__((availability(macos,introduced=13.0))) __attribute__((availability(ios,introduced=10.0)));
int vers1(void) __attribute__((availability(macos,introduced=10_14_4)));
int vers2(void) __attribute__((availability(macosx,introduced=12)));
int vers3(void) __attribute__((availability(macos,introduced=11.0.1)));
int t1(void) __attribute__((availability(macos,introduced=13.0))) __attribute__((availability(macos,introduced=14.0))) __attribute__((availability(macos,introduced=13.0)));
int t2(void) __attribute__((availability(macos,introduced=13.0))) __attribute__((availability(macos,introduced=14.0))) __attribute__((availability(macos,introduced=15.0)));
int t3(void) __attribute__((availability(macos,introduced=13.0)));
int t3(void) __attribute__((availability(macos,introduced=14.0)));
int t3(void) __attribute__((availability(macos,introduced=15.0)));
int t5(void) __attribute__((availability(macos,introduced=13.0,deprecated=14.0)));
int t5(void) __attribute__((availability(macos,introduced=13.0)));
int t6(void) __attribute__((availability(macos,introduced=13.0))) __attribute__((availability(macos,deprecated=14.0)));
int g(void)
{
    return two_a() + two_b() + two_c() + re_a() + re_c() + same() + u_then_i() + dep_then_intro()
        + mixed() + vers1() + vers2() + vers3() + t1() + t2() + t3() + t5() + t6();
}
