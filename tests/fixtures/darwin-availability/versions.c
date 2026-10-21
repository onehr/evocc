/* Versions as Clang reads them: `introduced=NA` is unavailable, a later
 * `deprecated=NA` has no effect, and `10.14.4` and `10_14_4` are versions. */
int ios_na(void) __attribute__((availability(ios,introduced=NA)));
int macos_na(void) __attribute__((availability(macos,introduced=NA)));
int never_deprecated(void) __attribute__((availability(macos,introduced=10.0,deprecated=NA)));
int dotted(void) __attribute__((availability(macos,introduced=10.14.4)));
int underscored(void) __attribute__((availability(macos,introduced=10_14_4,deprecated=12.0)));
int use(void)
{
    return ios_na() + macos_na() + never_deprecated() + dotted() + underscored();
}
