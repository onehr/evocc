#include "symbol-component.h"

int read_component(void) { return component_data + component_function(); }
int *component_address(void) { return &component_data; }
int *component_alias_address(void) { return &component_alias; }
int (*component_callable(void))(void) { return component_function; }
