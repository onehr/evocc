/* A directory also marked system must not keep its earlier -I priority. */
#include <selector.h>
int main(void) { return SELECTED_HEADER != 2; }
