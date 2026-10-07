# Use stock CMake compiler identification and build rules with EvoCC.
# Supply CMAKE_C_COMPILER when evocc is not on PATH. CMAKE_SYSROOT keeps its
# standard meaning; this file does not replace user compilation flags.
include_guard(GLOBAL)
list(APPEND CMAKE_TRY_COMPILE_PLATFORM_VARIABLES
     CMAKE_C_COMPILER CMAKE_SYSROOT EVOCC_TARGET
     CMAKE_TRY_COMPILE_TARGET_TYPE)

if(NOT CMAKE_C_COMPILER)
  find_program(CMAKE_C_COMPILER NAMES evocc REQUIRED)
endif()

# Identification runs before CMake knows the GNU profile's sysroot option.
# The existing driver environment supplies that root during identification.
# After identification, stock CMake supplies --sysroot on build commands.
if(CMAKE_SYSROOT)
  set(ENV{EVOCC_SYSROOT} "${CMAKE_SYSROOT}")
endif()

set(EVOCC_TARGET "" CACHE STRING "EvoCC target triple; empty uses the host")
if(EVOCC_TARGET STREQUAL "")
  return()
elseif(EVOCC_TARGET STREQUAL "aarch64-unknown-linux-gnu")
  set(_evocc_system Linux)
  set(_evocc_processor aarch64)
elseif(EVOCC_TARGET STREQUAL "aarch64-apple-darwin")
  set(_evocc_system Darwin)
  set(_evocc_processor arm64)
else()
  message(FATAL_ERROR "Unsupported EVOCC_TARGET: ${EVOCC_TARGET}")
endif()

# The GNU-compatible CMake profile needs the driver's explicit target spelling.
# Keep it separate from CMAKE_C_FLAGS so supplied user flags remain intact.
set(CMAKE_C_COMPILER_TARGET "${EVOCC_TARGET}")
set(CMAKE_C_COMPILE_OPTIONS_TARGET "--target=")
set(CMAKE_SYSTEM_PROCESSOR "${_evocc_processor}")
if(NOT CMAKE_HOST_SYSTEM_NAME STREQUAL _evocc_system)
  set(CMAKE_SYSTEM_NAME "${_evocc_system}")
  # Bundled Linux headers are not a link sysroot. Cross configuration checks
  # compile and archive; executable linking still requires an explicit linker.
  set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)
endif()
