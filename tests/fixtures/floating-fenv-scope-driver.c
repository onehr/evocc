#include <stdio.h>
extern int scope_probe(void), scope_goto(void), scope_boundaries(void), scope_computed(void);
extern int environment_probe(void), environment_arithmetic(void);
extern int scope_discarded(void);
extern int scope_expression(void);
int main(void) {
    int result=scope_probe(); if(result) {printf("scope_probe %d\n",result);return 1;}
    result=scope_goto(); if(result) {printf("scope_goto %d\n",result);return 2;}
    result=scope_boundaries(); if(result) {printf("scope_boundaries %d\n",result);return 3;}
    result=scope_computed(); if(result) {printf("scope_computed %d\n",result);return 4;}
    result=environment_probe(); if(result) {printf("environment_probe %d\n",result);return 5;}
    result=environment_arithmetic(); if(result) {printf("environment_arithmetic %d\n",result);return 6;}
    result=scope_discarded(); if(result) {printf("scope_discarded %d\n",result);return 7;}
    result=scope_expression(); if(result) {printf("scope_expression %d\n",result);return 8;}
    puts("fenv lexical scope: 8 programs PASS");
}
