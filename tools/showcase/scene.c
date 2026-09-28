/* Reference-built adapter; the parser/serializer linked below is EvoCC cJSON. */
#include "cJSON.h"
#include <stdio.h>
#include <stdlib.h>
int main(void)
{
    const char *source="{\"scene\":\"Neon harbor\",\"buildings\":["
      "{\"x\":0,\"height\":3,\"color\":\"#45c8d6\"},"
      "{\"x\":1,\"height\":7,\"color\":\"#e69b61\"},"
      "{\"x\":2,\"height\":5,\"color\":\"#9381ed\"},"
      "{\"x\":3,\"height\":9,\"color\":\"#45c8d6\"},"
      "{\"x\":4,\"height\":6,\"color\":\"#e69b61\"},"
      "{\"x\":5,\"height\":4,\"color\":\"#9381ed\"}]}";
    cJSON *root=cJSON_Parse(source);
    if(!root) return 1;
    cJSON *buildings=cJSON_GetObjectItemCaseSensitive(root,"buildings");
    int floors=0;
    for(int i=0;i<cJSON_GetArraySize(buildings);++i)
        floors+=cJSON_GetObjectItemCaseSensitive(cJSON_GetArrayItem(buildings,i),"height")->valueint;
    cJSON_AddNumberToObject(root,"floors",floors);
    char *encoded=cJSON_PrintUnformatted(root);
    if(!encoded || floors!=34) return 2;
    puts(encoded);
    cJSON_free(encoded);
    cJSON_Delete(root);
    return 0;
}
