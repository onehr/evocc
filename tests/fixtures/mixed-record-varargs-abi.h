struct small_mixed { int value; float unused; };
struct medium_mixed { long value; double unused; };
struct large_mixed { long first; double unused; long last; };
int read_mixed(unsigned skip, ...);
