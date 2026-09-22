#if VIS_KIND == 1
#define COMPONENT_VISIBILITY "protected"
#elif VIS_KIND == 2
#define COMPONENT_VISIBILITY "internal"
#elif VIS_KIND == 3
#define COMPONENT_VISIBILITY "hidden"
#else
#define COMPONENT_VISIBILITY "default"
#endif

#define COMPONENT_ATTRIBUTE __attribute__((visibility(COMPONENT_VISIBILITY)))
extern int component_data COMPONENT_ATTRIBUTE;
extern int component_function(void) __attribute__((noinline)) COMPONENT_ATTRIBUTE;
extern int component_alias COMPONENT_ATTRIBUTE;
