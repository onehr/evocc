#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "cJSON.h"

int main(void) {
    const char *text = "{\"name\":\"evocc\",\"items\":[1,2,3,4],\"ok\":true,\"nested\":{\"n\":1.25}}";
    unsigned i;
    for (i = 0; i < 4096; ++i) {
        cJSON *root = cJSON_Parse(text);
        cJSON *items, *copy;
        char *encoded;
        if (!root) return 2;
        items = cJSON_GetObjectItemCaseSensitive(root, "items");
        if (!cJSON_IsArray(items) || cJSON_GetArraySize(items) != 4) return 3;
        if (cJSON_GetArrayItem(items, 2)->valueint != 3) return 4;
        encoded = cJSON_PrintUnformatted(root);
        if (!encoded || strcmp(encoded, text)) return 5;
        copy = cJSON_Parse(encoded);
        if (!copy || !cJSON_Compare(root, copy, 1)) return 6;
        cJSON_Delete(copy);
        cJSON_Delete(root);
        free(encoded);
    }
    puts("json-roundtrip-ok");
    return 0;
}
