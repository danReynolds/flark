#ifndef SOVEREIGN_COMRAK_BRIDGE_H_
#define SOVEREIGN_COMRAK_BRIDGE_H_

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct SovereignComrakResponse {
  uint32_t abi_version;
  uint32_t revision;
  uint16_t status_code;
  uint16_t reserved;
  uint8_t* payload_ptr;
  uint32_t payload_len;
} SovereignComrakResponse;

uint32_t sovereign_comrak_bridge_version(void);

SovereignComrakResponse* sovereign_comrak_parse(
    uint32_t revision,
    uint8_t profile,
    const uint8_t* text_ptr,
    uint32_t text_len);

void sovereign_comrak_response_free(SovereignComrakResponse* response);

#ifdef __cplusplus
}  // extern "C"
#endif

#endif  // SOVEREIGN_COMRAK_BRIDGE_H_
