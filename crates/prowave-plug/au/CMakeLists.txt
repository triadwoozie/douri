# SPDX-License-Identifier: GPL-3.0-or-later
# Wraps a built .clap bundle as an Audio Unit (AUv2) .component with clap-wrapper
# (github.com/free-audio/clap-wrapper, MIT): the .clap is copied into the component's
# Contents/PlugIns and loaded from there. tools/build-plugins.sh runs this once per plug-in.
cmake_minimum_required(VERSION 3.21)
set(CMAKE_OSX_ARCHITECTURES "arm64;x86_64" CACHE STRING "")
set(CMAKE_OSX_DEPLOYMENT_TARGET "11.0" CACHE STRING "")
project(prowave-plug-au VERSION 1.0.0 LANGUAGES C CXX OBJC OBJCXX)
set(CMAKE_CXX_STANDARD 17)

set(CLAP_PATH "" CACHE PATH "Path to the .clap bundle to wrap")
set(AU_OUTPUT_NAME "" CACHE STRING "Bundle name (default: the .clap's)")
set(AU_TYPE "aumu" CACHE STRING "AU type: aumu (instrument) or aufx (effect)")
set(AU_SUBTYPE "" CACHE STRING "AU subtype (4 characters)")
set(AU_MANUFACTURER_CODE "PrWv" CACHE STRING "AU manufacturer code (4 characters)")
set(AU_MANUFACTURER_NAME "Pro Wave" CACHE STRING "AU manufacturer name")
set(AU_BUNDLE_ID "" CACHE STRING "Bundle identifier")
set(AU_VERSION "1.0.0" CACHE STRING "Bundle version")

if (NOT IS_DIRECTORY "${CLAP_PATH}")
  message(FATAL_ERROR "CLAP_PATH must point to an existing .clap bundle (got '${CLAP_PATH}')")
endif()
if (AU_OUTPUT_NAME STREQUAL "")
  get_filename_component(AU_OUTPUT_NAME "${CLAP_PATH}" NAME_WE)
endif()

set(CLAP_WRAPPER_DOWNLOAD_DEPENDENCIES ON CACHE BOOL "" FORCE)
set(CLAP_WRAPPER_BUILD_AUV2 ON CACHE BOOL "" FORCE)
set(CLAP_WRAPPER_DIR "" CACHE PATH "clap-wrapper source (github.com/free-audio/clap-wrapper)")
add_subdirectory(${CLAP_WRAPPER_DIR} clap-wrapper EXCLUDE_FROM_ALL)

add_library(wrapped_auv2 MODULE)
target_add_auv2_wrapper(
  TARGET wrapped_auv2
  OUTPUT_NAME "${AU_OUTPUT_NAME}"
  BUNDLE_IDENTIFIER "${AU_BUNDLE_ID}"
  BUNDLE_VERSION "${AU_VERSION}"
  MANUFACTURER_NAME "${AU_MANUFACTURER_NAME}"
  MANUFACTURER_CODE "${AU_MANUFACTURER_CODE}"
  SUBTYPE_CODE "${AU_SUBTYPE}"
  INSTRUMENT_TYPE "${AU_TYPE}"
  MACOS_EMBEDDED_CLAP_LOCATION "${CLAP_PATH}"
)
