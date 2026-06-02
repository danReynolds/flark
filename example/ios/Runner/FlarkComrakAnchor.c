#include "../../../native/comrak_bridge/flark_comrak_bridge.h"

__attribute__((used)) void* flark_comrak_anchor_symbols[] = {
    (void*)flark_comrak_bridge_version,
    (void*)flark_comrak_input_alloc,
    (void*)flark_comrak_input_free,
    (void*)flark_comrak_parse,
    (void*)flark_comrak_response_free,
};
