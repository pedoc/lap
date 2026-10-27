cmake_minimum_required(VERSION 3.13)
project(LapUpnp LANGUAGES C)

get_filename_component(LAP_THIRD_PARTY "${CMAKE_CURRENT_SOURCE_DIR}/../../third_party" ABSOLUTE)
if(MSVC)
    enable_language(CXX)
    # netdev supplies Windows adapter GUIDs; do not convert localized friendly names.
    set(UPNP_ADAPTER_UUID_NAME ON CACHE BOOL "" FORCE)
    set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
    # Upstream detects architecture by running an executable. Supply its known
    # ARM64 result when cross-compiling from an x64 Windows host.
    string(TOUPPER "${CMAKE_GENERATOR_PLATFORM}" lap_target_platform)
    if(lap_target_platform STREQUAL "ARM64" OR CMAKE_C_COMPILER_ARCHITECTURE_ID STREQUAL "ARM64")
        set(lap_original_crosscompiling "${CMAKE_CROSSCOMPILING}")
        set(CMAKE_CROSSCOMPILING TRUE)
        set(run_result 3 CACHE STRING "pthreads4w ARM64 architecture probe" FORCE)
    endif()
    add_subdirectory("${LAP_THIRD_PARTY}/pthreads4w" pthreads EXCLUDE_FROM_ALL)
    if(DEFINED lap_original_crosscompiling)
        set(CMAKE_CROSSCOMPILING "${lap_original_crosscompiling}")
    endif()
    # The SDK expects these imported targets. Keep its source unmodified.
    add_library(Threads::Static STATIC IMPORTED GLOBAL)
    set_target_properties(Threads::Static PROPERTIES
        IMPORTED_LOCATION "${CMAKE_CURRENT_BINARY_DIR}/pthreads/Release/pthreadVC3.lib"
        INTERFACE_INCLUDE_DIRECTORIES "${LAP_THIRD_PARTY}/pthreads4w"
        INTERFACE_COMPILE_DEFINITIONS PTW32_STATIC_LIB)
    add_dependencies(Threads::Static pthreadVC3)
    set(PTHREADS4W_DIR "${CMAKE_CURRENT_SOURCE_DIR}")
    add_compile_options(/utf-8)
endif()

set(UPNP_BUILD_SHARED OFF CACHE BOOL "" FORCE)
set(UPNP_BUILD_STATIC ON CACHE BOOL "" FORCE)
set(UPNP_BUILD_SAMPLES OFF CACHE BOOL "" FORCE)
set(UPNP_ENABLE_TESTING OFF CACHE BOOL "" FORCE)
set(UPNP_ENABLE_TESTING_INTEGRATION OFF CACHE BOOL "" FORCE)
set(UPNP_ENABLE_IPV6 OFF CACHE BOOL "" FORCE)
set(UPNP_ENABLE_DEBUG OFF CACHE BOOL "" FORCE)
set(UPNP_ENABLE_BACKTRACE OFF CACHE BOOL "" FORCE)
add_subdirectory("${LAP_THIRD_PARTY}/libupnp" sdk)

# Keep compatibility/policy changes out of the pinned upstream checkout.
function(lap_replace_source relative content)
    get_filename_component(name "${relative}" NAME)
    set(generated "${CMAKE_CURRENT_BINARY_DIR}/lap-${name}")
    if(EXISTS "${generated}")
        file(READ "${generated}" previous_content)
    endif()
    if(NOT previous_content STREQUAL content)
        file(WRITE "${generated}" "${content}")
    endif()
    get_target_property(sources upnp_static SOURCES)
    string(REPLACE "${relative}" "${generated}" sources "${sources}")
    set_property(TARGET upnp_static PROPERTY SOURCES "${sources}")
endfunction()
if(MSVC)
    # pthreads4w renamed its export macro; the SDK's logging fallback assumes
    # pthread_t is an integer. Use the actual Windows thread ID for logging.
    file(READ "${LAP_THIRD_PARTY}/libupnp/upnp/src/api/upnpdebug.c" debug_source)
    string(REPLACE "(unsigned long int)ithread_self()" "GetCurrentThreadId()" debug_source "${debug_source}")
    lap_replace_source("src/api/upnpdebug.c" "${debug_source}")
endif()
# A photo-library subscription may only call back the subscribing IPv4 peer.
# The SDK default also permits other RFC1918 hosts. Restrict that policy while
# leaving its HTTP, GENA, expiry, sequencing, and notification handling intact.
file(READ "${LAP_THIRD_PARTY}/libupnp/upnp/src/gena/gena_device.c" gena_source)
set(policy_anchor "switch (info->foreign_sockaddr.ss_family) {")
set(peer_policy [=[
if (info->foreign_sockaddr.ss_family != AF_INET || url_list->size != 1 ||
    url_list->parsedURLs[0].hostport.IPaddress.ss_family != AF_INET ||
    ((struct sockaddr_in *)&url_list->parsedURLs[0].hostport.IPaddress)->sin_addr.s_addr !=
    ((struct sockaddr_in *)&info->foreign_sockaddr)->sin_addr.s_addr) return -1;
]=])
string(FIND "${gena_source}" "${policy_anchor}" anchor_position)
if(anchor_position EQUAL -1)
    message(FATAL_ERROR "Review the GENA callback policy before updating libupnp")
endif()
string(REPLACE "${policy_anchor}" "${peer_policy}\n${policy_anchor}" gena_source "${gena_source}")
lap_replace_source("src/gena/gena_device.c" "${gena_source}")

add_library(lap_upnp STATIC bridge.c)
target_link_libraries(lap_upnp PRIVATE UPNP::Static)
target_compile_definitions(lap_upnp PRIVATE UPNP_STATIC_LIB)
install(TARGETS lap_upnp ARCHIVE DESTINATION lib)
if(MSVC)
    install(TARGETS pthreadVC3 ARCHIVE DESTINATION lib)
endif()
