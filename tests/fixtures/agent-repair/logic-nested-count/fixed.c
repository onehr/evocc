/* Adapted from TinyCC tests under GNU LGPL v2.1; see ../LICENSE.tinycc.
 * Modified for the EvoCC agent-repair corpus on 2026-10-07.
 */

int grid_count(int width, int height, int depth);
int main(void);

/* Return the number of (x,y,z) cells with 0 <= x < width, 0 <= y < height, and 0 <= z < depth. Dimensions must be in [0, 10]; otherwise return 0. */
int grid_count(int width, int height, int depth)
{
    int x;
    int y;
    int z;
    int count = 0;
    if (width < 0 || width > 10 || height < 0 || height > 10 || depth < 0 || depth > 10)
        return 0;
    for (x = 0; x < width; x++) {
        for (y = 0; y < height; y++) {
            for (z = 0; z < depth; z++)
                count++;
        }
    }
    return count;
}

int main(void)
{
    return grid_count(2, 3, 3) != 18;
}
