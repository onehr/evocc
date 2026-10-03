/* Export may preserve this syntax; independent source validation must refuse.
 * Never execute this undefined C program as a numerical reference.
 */
int unsequenced(int x) { return x++ + x; }
