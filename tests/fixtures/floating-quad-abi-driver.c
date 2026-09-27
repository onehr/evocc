#include <stdio.h>
#include "floating-quad-abi.h"
void qa_clobber(void) {
    /* AAPCS preserves only the low 64 bits of these registers. */
    __asm__ volatile("movi v8.16b,#85; movi v9.16b,#102; movi v10.16b,#119; movi v11.16b,#136; movi v12.16b,#153; movi v13.16b,#170; movi v14.16b,#187; movi v15.16b,#204"
        ::: "v8","v9","v10","v11","v12","v13","v14","v15");
}
int main(void) { int result=qa_caller(); printf("quad ABI: %s (%d)\n",result?"FAIL":"PASS",result); return result; }
