# The Flutter tooling requires that developers have CMake 3.10 or later
# installed. You should not increase this version, as doing so will cause
# the plugin to fail to compile for some customers of the plugin.
cmake_minimum_required(VERSION 3.10)

project(flark_spike_ffi_library VERSION 0.0.1 LANGUAGES C)

add_library(flark_spike_ffi SHARED
  "flark_spike_ffi.c"
)

set_target_properties(flark_spike_ffi PROPERTIES
  PUBLIC_HEADER flark_spike_ffi.h
  OUTPUT_NAME "flark_spike_ffi"
)

target_compile_definitions(flark_spike_ffi PUBLIC DART_SHARED_LIB)

if (ANDROID)
  # Support Android 15 16k page size
  target_link_options(flark_spike_ffi PRIVATE "-Wl,-z,max-page-size=16384")
endif()
