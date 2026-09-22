/* These definitions are in the executable, not the tested DSO. */
int component_data = 90;
int component_alias = 99;
int component_function(void) { return 91; }

int read_definition(void);
int read_component(void);
int *component_address(void);
int *component_alias_address(void);
int *definition_alias_address(void);
int (*component_callable(void))(void);

int main(void) {
    if (component_data != 90 || component_function() != 91) return 1;
#if VIS_KIND == 4
    if (read_component() != 181 || read_definition() != 181) return 2;
    if (component_address() != &component_data) return 3;
    if (component_alias_address() != &component_alias) return 4;
    if (definition_alias_address() != &component_alias) return 10;
    if (component_callable() != component_function) return 5;
#else
    if (read_component() != 22 || read_definition() != 22) return 6;
    if (component_address() == &component_data || *component_address() != 11) return 7;
    if (component_alias_address() != component_address()) return 8;
    if (definition_alias_address() != component_address()) return 11;
    if (component_callable() == component_function || component_callable()() != 11) return 9;
#endif
    return 0;
}
