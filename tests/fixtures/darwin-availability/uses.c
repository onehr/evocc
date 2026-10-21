/* Uses of unavailable declarations (EvoIR 10.1.9). The test lists the
 * uses that Apple clang 21 with -mmacosx-version-min=11.0 refuses. */
int gone(void) __attribute__((availability(macos,unavailable)));
int obs(void) __attribute__((availability(macos,introduced=10.0,obsoleted=10.15)));
int later(void) __attribute__((availability(macos,introduced=13.0)));
int strict_later(void) __attribute__((availability(macos,strict,introduced=13.0)));
int dep(void) __attribute__((availability(macos,introduced=10.0,deprecated=10.12,message="use other")));
int ios_only(void) __attribute__((availability(ios,unavailable)));
int plain_unavail(void) __attribute__((unavailable));
int msg_unavail(void) __attribute__((unavailable("no way")));
int wi(void) __attribute__((weak_import));
extern int later_object __attribute__((availability(macos,introduced=13.0)));
int f1(void) { return gone(); }
int f2(void) { return obs(); }
int f3(void) { return later(); }
int f4(void) { return strict_later(); }
int f5(void) { return dep(); }
int f6(void) { return ios_only(); }
int f7(void) { return plain_unavail(); }
int f8(void) { return msg_unavail(); }
int f9(void) { return wi(); }
int f10(void) { return later_object; }
int f11(void) __attribute__((availability(macos,unavailable)));
int f11(void) { return gone(); }
int (*p)(void) = gone;
int f12(void) { return sizeof(gone()); }
