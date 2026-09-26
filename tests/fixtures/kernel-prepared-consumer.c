/* T04: the generated input, not an old reference header, determines the value.
 * Compile with -DRESUME_NAME=consumer_N. The recovery test generates the header
 * and uses real EvoCC assembly plus the sealed assembler for each consumer. */
#include "prepared-offsets.h"

int RESUME_NAME(void)
{
    return PREPARED_OFFSET;
}
