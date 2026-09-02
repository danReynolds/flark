#include "flark_v4.h"

int flark_v4_header_smoke(void) {
  FlarkV4SmallEditRequest request = {0};
  FlarkV4EditDescriptor edit = {0};
  FlarkV4Outcome outcome = {0};
  FlarkV4ResultPageHeader page = {0};
  FlarkV4SessionInspection inspection = {0};
  request.struct_size = FLARK_V4_SIZEOF_SMALL_EDIT_REQUEST;
  request.edit_count = 1;
  edit.end_byte = 1;
  outcome.operation = FLARK_V4_OPERATION_SMALL_EDIT;
  outcome.status = FLARK_V4_STATUS_OK;
  page.struct_size = FLARK_V4_SIZEOF_RESULT_PAGE_HEADER;
  page.abi_major = FLARK_V4_ABI_MAJOR;
  page.record_kind = FLARK_V4_RESULT_RECORD_SOURCE_BYTES;
  page.certification_state = FLARK_V4_CERTIFICATION_NOT_APPLICABLE;
  inspection.struct_size = FLARK_V4_SIZEOF_SESSION_INSPECTION;
  inspection.session_state = FLARK_V4_SESSION_OPEN;
  return (int)(request.edit_count + edit.end_byte + outcome.status +
               page.struct_size + inspection.session_state);
}
