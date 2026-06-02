#include "../../../native/comrak_bridge/sovereign_comrak_bridge.h"

__attribute__((used)) void* sovereign_comrak_anchor_symbols[] = {
    (void*)sovereign_comrak_bridge_version,
    (void*)sovereign_comrak_input_alloc,
    (void*)sovereign_comrak_input_free,
    (void*)sovereign_comrak_parse,
    (void*)sovereign_comrak_response_free,
};
