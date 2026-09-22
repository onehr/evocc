#include "symbol-component.h"

int component_data = 11;
int component_function(void) { return component_data; }
extern int component_alias
    __attribute__((weak, alias("component_data"))) COMPONENT_ATTRIBUTE;

/* This definition's own references must also obey dynamic interposition. */
int read_definition(void) { return component_data + component_function(); }
int *definition_alias_address(void) { return &component_alias; }
