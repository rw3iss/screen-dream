cmake_minimum_required(VERSION 3.20)
project(ScreenDream LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)

find_package(Qt6 REQUIRED COMPONENTS Widgets Core Gui)

# --- Rust FFI library -----------------------------------------------------------

set(RUST_DIR "${CMAKE_SOURCE_DIR}/../rust-core")

# Choose cargo profile based on CMake build type
if(CMAKE_BUILD_TYPE STREQUAL "Release")
    set(CARGO_PROFILE "release")
    set(CARGO_RELEASE_FLAG "--release")
else()
    set(CARGO_PROFILE "debug")
    set(CARGO_RELEASE_FLAG "")
endif()

set(RUST_LIB_DIR "${RUST_DIR}/target/${CARGO_PROFILE}")
set(RUST_FFI_LIB "${RUST_LIB_DIR}/libscreen_dream_ffi.so")

add_custom_command(
    OUTPUT "${RUST_FFI_LIB}"
    COMMAND cargo build --manifest-path "${RUST_DIR}/Cargo.toml" -p ffi ${CARGO_RELEASE_FLAG}
    COMMENT "Building Rust FFI library (${CARGO_PROFILE})"
    VERBATIM
)
add_custom_target(rust_ffi ALL DEPENDS "${RUST_FFI_LIB}")

# Link against the Rust FFI library by name (not absolute path)
# so that setcap CAP_SYS_ADMIN works (AT_SECURE blocks absolute paths)
link_directories("${RUST_LIB_DIR}")

# --- Qt application -------------------------------------------------------------

set(SOURCES
    src/main.cpp
    src/core/RustBridge.cpp
    src/core/AppState.cpp
    src/ui/MainWindow.cpp
    src/widgets/CaptureCard.cpp
    src/widgets/RecentCaptures.cpp
    src/widgets/SourceBrowser.cpp
)

set(HEADERS
    src/core/RustBridge.h
    src/core/AppState.h
    src/ui/MainWindow.h
    src/widgets/CaptureCard.h
    src/widgets/RecentCaptures.h
    src/widgets/SourceBrowser.h
)

set(RESOURCES
    src/resources/resources.qrc
)

add_executable(ScreenDream ${SOURCES} ${HEADERS} ${RESOURCES})
add_dependencies(ScreenDream rust_ffi)

# Also link libdrm for DRM framebuffer capture
find_library(DRM_LIB drm)
if(DRM_LIB)
    target_link_libraries(ScreenDream PRIVATE ${DRM_LIB})
endif()

target_include_directories(ScreenDream PRIVATE
    "${RUST_DIR}/crates/ffi"       # screen_dream_ffi.h
    "${CMAKE_SOURCE_DIR}/src"
)

target_link_libraries(ScreenDream PRIVATE
    Qt6::Widgets
    Qt6::Core
    Qt6::Gui
    -lscreen_dream_ffi
)

# RPATH so the binary can find libscreen_dream_ffi.so at runtime
set_target_properties(ScreenDream PROPERTIES
    BUILD_RPATH "${RUST_LIB_DIR}"
    INSTALL_RPATH "${RUST_LIB_DIR}"
)
