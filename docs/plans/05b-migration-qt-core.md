# Plan 5b: Qt6 Migration — C++ Bridge Layer & Core Windows

> **License:** GPLv3

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**App Name:** Screen Dream

**Goal:** Build the C++ bridge layer that wraps the Rust FFI library (`libscreen_dream_ffi`) in safe, idiomatic C++ with RAII semantics, then stand up the Qt6 project with CMake, the application entry point, dark theme, and the core UI shell (MainWindow, CaptureCard, RecentCaptures).

**Architecture:** Qt6 C++ desktop application backed by a Rust core library exposed via C FFI. The `RustBridge` class owns the FFI lifecycle and translates all C types into Qt/C++ types. `AppState` is a singleton QObject providing signals for state changes. The UI layer is pure Qt Widgets styled with a QSS dark theme.

**Tech Stack:** Qt 6.5+, C++17, CMake 3.20+, Rust (via cargo + cbindgen), FFmpeg (system)

**Related documents:**
- `docs/plans/05a-migration-ffi-layer.md` — Plan 5a (Rust FFI crate — prerequisite)
- `docs/plans/01-core-platform-infrastructure.md` — Original Tauri architecture
- `docs/plans/02-screen-capture-recording.md` — Capture/recording features to replicate

**Phases covered:** Phase 3 (C++ Bridge Layer) and Phase 4 (Qt6 Project Setup & Core Windows)

---

## Directory Structure

```
qt-ui/
├── CMakeLists.txt
├── src/
│   ├── main.cpp
│   ├── core/
│   │   ├── RustBridge.h
│   │   ├── RustBridge.cpp
│   │   ├── AppState.h
│   │   └── AppState.cpp
│   ├── ui/
│   │   ├── MainWindow.h
│   │   ├── MainWindow.cpp
│   │   ├── RecorderPanel.h        (future)
│   │   ├── SourcePickerDialog.h   (future)
│   │   ├── RegionSelector.h       (future)
│   │   ├── EditorWindow.h         (future)
│   │   └── SettingsDialog.h       (future)
│   ├── widgets/
│   │   ├── CaptureCard.h
│   │   ├── CaptureCard.cpp
│   │   ├── RecentCaptures.h
│   │   ├── RecentCaptures.cpp
│   │   ├── FlowLayout.h
│   │   ├── FlowLayout.cpp
│   │   ├── VideoPreview.h          (future)
│   │   └── Timeline.h              (future)
│   └── resources/
│       ├── styles/dark-theme.qss
│       ├── icons/
│       └── resources.qrc
├── include/
│   └── screen_dream_ffi.h
```

---

## Phase 3: C++ Bridge Layer

---

### Task 9: CMakeLists.txt — Qt6 + Rust Integration

**File:** `qt-ui/CMakeLists.txt`

#### Steps

- [ ] **9.1** Create the `qt-ui/` directory and `CMakeLists.txt`
- [ ] **9.2** Set minimum CMake version, project name, C++17 standard
- [ ] **9.3** Find Qt6 components (Widgets, Core, Gui, OpenGLWidgets)
- [ ] **9.4** Add custom command to build Rust FFI library via cargo
- [ ] **9.5** Set platform-conditional library name (`.so` / `.dylib` / `.dll`)
- [ ] **9.6** Define all C++ source files and create the executable target
- [ ] **9.7** Link Qt6 libraries and the Rust FFI library
- [ ] **9.8** Enable AUTOMOC, AUTOUIC, AUTORCC
- [ ] **9.9** Verify build with `cmake -B build && cmake --build build`

#### Complete File

```cmake
# qt-ui/CMakeLists.txt
cmake_minimum_required(VERSION 3.20)
project(ScreenDream VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTOUIC ON)
set(CMAKE_AUTORCC ON)

# ── Qt6 ──────────────────────────────────────────────────────────────────────
find_package(Qt6 REQUIRED COMPONENTS Widgets Core Gui OpenGLWidgets)

# ── Rust FFI Library ─────────────────────────────────────────────────────────
set(RUST_PROJECT_DIR "${CMAKE_CURRENT_SOURCE_DIR}/..")
set(RUST_TARGET_DIR "${RUST_PROJECT_DIR}/target/release")

# Platform-specific library name
if(WIN32)
    set(FFI_LIB_NAME "screen_dream_ffi.dll")
    set(FFI_IMPORT_LIB "${RUST_TARGET_DIR}/screen_dream_ffi.dll.lib")
elseif(APPLE)
    set(FFI_LIB_NAME "libscreen_dream_ffi.dylib")
else()
    set(FFI_LIB_NAME "libscreen_dream_ffi.so")
endif()

set(FFI_LIB_PATH "${RUST_TARGET_DIR}/${FFI_LIB_NAME}")

# Custom command: build the Rust FFI crate
add_custom_command(
    OUTPUT "${FFI_LIB_PATH}"
    COMMAND cargo build --release -p ffi
    WORKING_DIRECTORY "${RUST_PROJECT_DIR}"
    COMMENT "Building Rust FFI library (cargo build --release -p ffi)"
    VERBATIM
)

add_custom_target(rust_ffi ALL DEPENDS "${FFI_LIB_PATH}")

# Import the shared library
add_library(screen_dream_ffi SHARED IMPORTED)
set_target_properties(screen_dream_ffi PROPERTIES
    IMPORTED_LOCATION "${FFI_LIB_PATH}"
)
if(WIN32)
    set_target_properties(screen_dream_ffi PROPERTIES
        IMPORTED_IMPLIB "${FFI_IMPORT_LIB}"
    )
endif()

# ── Sources ──────────────────────────────────────────────────────────────────
set(SOURCES
    src/main.cpp
    src/core/RustBridge.cpp
    src/core/AppState.cpp
    src/ui/MainWindow.cpp
    src/widgets/CaptureCard.cpp
    src/widgets/RecentCaptures.cpp
    src/widgets/FlowLayout.cpp
)

set(HEADERS
    src/core/RustBridge.h
    src/core/AppState.h
    src/ui/MainWindow.h
    src/widgets/CaptureCard.h
    src/widgets/RecentCaptures.h
    src/widgets/FlowLayout.h
)

set(RESOURCES
    src/resources/resources.qrc
)

# ── Executable ───────────────────────────────────────────────────────────────
add_executable(${PROJECT_NAME}
    ${SOURCES}
    ${HEADERS}
    ${RESOURCES}
)

add_dependencies(${PROJECT_NAME} rust_ffi)

target_include_directories(${PROJECT_NAME} PRIVATE
    "${CMAKE_CURRENT_SOURCE_DIR}/include"
    "${CMAKE_CURRENT_SOURCE_DIR}/src"
)

target_link_libraries(${PROJECT_NAME} PRIVATE
    Qt6::Widgets
    Qt6::Core
    Qt6::Gui
    Qt6::OpenGLWidgets
    screen_dream_ffi
)

# ── RPATH (Linux/macOS: find .so/.dylib next to binary or in target/release) ─
if(UNIX AND NOT APPLE)
    set_target_properties(${PROJECT_NAME} PROPERTIES
        INSTALL_RPATH "$ORIGIN;${RUST_TARGET_DIR}"
        BUILD_RPATH "${RUST_TARGET_DIR}"
    )
elseif(APPLE)
    set_target_properties(${PROJECT_NAME} PROPERTIES
        INSTALL_RPATH "@executable_path;${RUST_TARGET_DIR}"
        BUILD_RPATH "${RUST_TARGET_DIR}"
    )
endif()

# ── Install ──────────────────────────────────────────────────────────────────
install(TARGETS ${PROJECT_NAME} RUNTIME DESTINATION bin)
install(FILES "${FFI_LIB_PATH}" DESTINATION lib)
```

#### Build Commands

```bash
cd qt-ui
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel $(nproc)
./build/ScreenDream
```

---

### Task 10: RustBridge — C++ RAII Wrapper

**Files:** `qt-ui/src/core/RustBridge.h`, `qt-ui/src/core/RustBridge.cpp`

#### Steps

- [ ] **10.1** Create `RustBridge.h` with C++ struct definitions mirroring FFI types
- [ ] **10.2** Define the `RustBridge` class with constructor (`sd_init`) and destructor (`sd_shutdown`)
- [ ] **10.3** Implement all wrapper methods returning Qt/C++ types
- [ ] **10.4** Implement error-handling helper that converts `SDError*` to exceptions
- [ ] **10.5** Implement `RustBridge.cpp` with full method bodies
- [ ] **10.6** Unit test: instantiate RustBridge, call `getPlatformInfo()`

#### RustBridge.h

```cpp
// qt-ui/src/core/RustBridge.h
#pragma once

#include <QString>
#include <QVector>
#include <QImage>
#include <QJsonObject>
#include <QJsonArray>
#include <QJsonDocument>
#include <QByteArray>
#include <QStandardPaths>

#include <optional>
#include <stdexcept>
#include <memory>

// Forward-declare the C FFI types (included from screen_dream_ffi.h)
extern "C" {
#include "screen_dream_ffi.h"
}

namespace sd {

// ── C++ mirror structs ──────────────────────────────────────────────────────

struct PlatformInfo {
    QString os;
    QString osVersion;
    QString desktopEnvironment;
    QString displayServer;
    bool compositorRunning;
};

struct MonitorInfo {
    uint32_t id;
    QString name;
    int32_t x;
    int32_t y;
    uint32_t width;
    uint32_t height;
    double scaleFactor;
    bool isPrimary;
};

struct WindowInfo {
    uint64_t id;
    QString title;
    QString appName;
    bool isMinimized;
};

struct AvailableSources {
    QVector<MonitorInfo> monitors;
    QVector<WindowInfo> windows;
};

struct CaptureSource {
    enum Type { FullScreen, Monitor, Window, Region };
    Type type;
    uint64_t sourceId;        // monitor id or window id
    // Region fields (only used when type == Region)
    int32_t regionX = 0;
    int32_t regionY = 0;
    uint32_t regionWidth = 0;
    uint32_t regionHeight = 0;
};

struct RecordingConfig {
    CaptureSource source;
    QString outputDir;
    QString format;           // "mp4", "webm", "mkv"
    uint32_t fps = 30;
    uint32_t videoBitrate = 8000;
    bool captureAudio = false;
    QString audioDevice;
    bool captureMicrophone = false;
    QString microphoneDevice;
};

struct RecordingStatus {
    bool isRecording;
    bool isPaused;
    double durationSecs;
    uint64_t bytesWritten;
};

struct FfmpegStatus {
    bool available;
    QString version;
    QString path;
    QVector<QString> encoders;
};

struct AudioDeviceInfo {
    QString id;
    QString name;
    bool isDefault;
    bool isInput;
};

// ── RustBridge class ────────────────────────────────────────────────────────

class RustBridge {
public:
    /// Initializes the Rust backend. Throws std::runtime_error on failure.
    explicit RustBridge(const QString& configDir = QString());

    /// Shuts down the Rust backend.
    ~RustBridge();

    // Non-copyable, movable
    RustBridge(const RustBridge&) = delete;
    RustBridge& operator=(const RustBridge&) = delete;
    RustBridge(RustBridge&& other) noexcept;
    RustBridge& operator=(RustBridge&& other) noexcept;

    // ── Platform ────────────────────────────────────────────────────────
    PlatformInfo getPlatformInfo();

    // ── Sources ─────────────────────────────────────────────────────────
    AvailableSources enumerateSources();

    // ── Screenshots ─────────────────────────────────────────────────────
    QString takeScreenshot(const CaptureSource& source, const QString& path);
    QByteArray takeScreenshotBase64(const CaptureSource& source);

    // ── Frame capture ───────────────────────────────────────────────────
    QImage captureFrame(const CaptureSource& source);

    // ── Recording ───────────────────────────────────────────────────────
    SDRecordingHandle* startRecording(const RecordingConfig& config);
    QString stopRecording(SDRecordingHandle* handle);
    void pauseRecording(SDRecordingHandle* handle);
    void resumeRecording(SDRecordingHandle* handle);
    RecordingStatus getRecordingStatus(SDRecordingHandle* handle);

    // ── Settings ────────────────────────────────────────────────────────
    QJsonObject loadSettings();
    void saveSettings(const QJsonObject& settings);

    // ── FFmpeg ──────────────────────────────────────────────────────────
    FfmpegStatus getFfmpegStatus();

    // ── Audio devices ───────────────────────────────────────────────────
    QVector<AudioDeviceInfo> listAudioDevices();

private:
    bool m_initialized = false;

    /// Converts SDError* to a std::runtime_error and frees the error.
    /// If error is null, does nothing.
    void checkError(SDError* error, const char* context);

    /// Converts a CaptureSource to the FFI SDCaptureSource struct.
    SDCaptureSource toFfi(const CaptureSource& source);
};

} // namespace sd
```

#### RustBridge.cpp

```cpp
// qt-ui/src/core/RustBridge.cpp
#include "core/RustBridge.h"

#include <QJsonDocument>
#include <QJsonArray>
#include <QJsonObject>
#include <QDebug>

namespace sd {

// ── Helpers ─────────────────────────────────────────────────────────────────

void RustBridge::checkError(SDError* error, const char* context) {
    if (!error) return;

    QString msg = QString("[%1] %2 (code %3)")
        .arg(context)
        .arg(error->message ? QString::fromUtf8(error->message) : "Unknown error")
        .arg(error->code);

    sd_free_error(error);
    throw std::runtime_error(msg.toStdString());
}

SDCaptureSource RustBridge::toFfi(const CaptureSource& source) {
    SDCaptureSource ffi{};
    switch (source.type) {
        case CaptureSource::FullScreen:
            ffi.source_type = SD_SOURCE_FULL_SCREEN;
            break;
        case CaptureSource::Monitor:
            ffi.source_type = SD_SOURCE_MONITOR;
            ffi.source_id = source.sourceId;
            break;
        case CaptureSource::Window:
            ffi.source_type = SD_SOURCE_WINDOW;
            ffi.source_id = source.sourceId;
            break;
        case CaptureSource::Region:
            ffi.source_type = SD_SOURCE_REGION;
            ffi.region_x = source.regionX;
            ffi.region_y = source.regionY;
            ffi.region_width = source.regionWidth;
            ffi.region_height = source.regionHeight;
            break;
    }
    return ffi;
}

// ── Constructor / Destructor ────────────────────────────────────────────────

RustBridge::RustBridge(const QString& configDir) {
    QString dir = configDir.isEmpty()
        ? QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation)
        : configDir;

    QByteArray dirUtf8 = dir.toUtf8();
    SDError* error = nullptr;
    sd_init(dirUtf8.constData(), &error);
    if (error) {
        checkError(error, "sd_init");
        return; // unreachable — checkError throws
    }
    m_initialized = true;
}

RustBridge::~RustBridge() {
    if (m_initialized) {
        sd_shutdown();
        m_initialized = false;
    }
}

RustBridge::RustBridge(RustBridge&& other) noexcept
    : m_initialized(other.m_initialized)
{
    other.m_initialized = false;
}

RustBridge& RustBridge::operator=(RustBridge&& other) noexcept {
    if (this != &other) {
        if (m_initialized) {
            sd_shutdown();
        }
        m_initialized = other.m_initialized;
        other.m_initialized = false;
    }
    return *this;
}

// ── Platform ────────────────────────────────────────────────────────────────

PlatformInfo RustBridge::getPlatformInfo() {
    SDPlatformInfo raw = sd_get_platform_info();
    PlatformInfo info;
    info.os = QString::fromUtf8(raw.os);
    info.osVersion = QString::fromUtf8(raw.os_version);
    info.desktopEnvironment = QString::fromUtf8(raw.desktop_environment);
    info.displayServer = QString::fromUtf8(raw.display_server);
    info.compositorRunning = raw.compositor_running;
    // Note: sd_get_platform_info returns a stack struct with static strings;
    // no freeing required per the FFI contract.
    return info;
}

// ── Sources ─────────────────────────────────────────────────────────────────

AvailableSources RustBridge::enumerateSources() {
    SDError* error = nullptr;
    SDAvailableSources* raw = sd_enumerate_sources(&error);
    checkError(error, "sd_enumerate_sources");

    AvailableSources result;

    if (raw) {
        for (size_t i = 0; i < raw->monitor_count; ++i) {
            const SDMonitorInfo& m = raw->monitors[i];
            MonitorInfo info;
            info.id = m.id;
            info.name = QString::fromUtf8(m.name);
            info.x = m.x;
            info.y = m.y;
            info.width = m.width;
            info.height = m.height;
            info.scaleFactor = m.scale_factor;
            info.isPrimary = m.is_primary;
            result.monitors.append(info);
        }

        for (size_t i = 0; i < raw->window_count; ++i) {
            const SDWindowInfo& w = raw->windows[i];
            WindowInfo info;
            info.id = w.id;
            info.title = QString::fromUtf8(w.title);
            info.appName = QString::fromUtf8(w.app_name);
            info.isMinimized = w.is_minimized;
            result.windows.append(info);
        }

        sd_free_available_sources(raw);
    }

    return result;
}

// ── Screenshots ─────────────────────────────────────────────────────────────

QString RustBridge::takeScreenshot(const CaptureSource& source, const QString& path) {
    SDCaptureSource ffiSource = toFfi(source);
    QByteArray pathUtf8 = path.toUtf8();
    SDError* error = nullptr;

    sd_take_screenshot(ffiSource, pathUtf8.constData(), &error);
    checkError(error, "sd_take_screenshot");

    return path;
}

QByteArray RustBridge::takeScreenshotBase64(const CaptureSource& source) {
    SDCaptureSource ffiSource = toFfi(source);
    SDError* error = nullptr;

    char* base64 = sd_take_screenshot_base64(ffiSource, &error);
    checkError(error, "sd_take_screenshot_base64");

    QByteArray result;
    if (base64) {
        result = QByteArray::fromBase64(QByteArray(base64));
        sd_free_string(base64);
    }
    return result;
}

// ── Frame Capture ───────────────────────────────────────────────────────────

QImage RustBridge::captureFrame(const CaptureSource& source) {
    SDCaptureSource ffiSource = toFfi(source);
    SDError* error = nullptr;

    SDFrame* frame = sd_capture_frame(ffiSource, &error);
    checkError(error, "sd_capture_frame");

    if (!frame || !frame->data) {
        throw std::runtime_error("sd_capture_frame returned null frame");
    }

    // SDFrame provides RGBA pixel data
    QImage image(
        frame->data,
        static_cast<int>(frame->width),
        static_cast<int>(frame->height),
        static_cast<int>(frame->stride),
        QImage::Format_RGBA8888
    );

    // Deep copy before freeing the FFI frame
    QImage result = image.copy();
    sd_free_frame(frame);

    return result;
}

// ── Recording ───────────────────────────────────────────────────────────────

SDRecordingHandle* RustBridge::startRecording(const RecordingConfig& config) {
    SDRecordingConfig ffiConfig{};
    ffiConfig.source = toFfi(config.source);

    QByteArray outputDirUtf8 = config.outputDir.toUtf8();
    ffiConfig.output_dir = outputDirUtf8.constData();

    QByteArray formatUtf8 = config.format.toUtf8();
    ffiConfig.format = formatUtf8.constData();

    ffiConfig.fps = config.fps;
    ffiConfig.video_bitrate = config.videoBitrate;
    ffiConfig.capture_audio = config.captureAudio;

    QByteArray audioDevUtf8 = config.audioDevice.toUtf8();
    ffiConfig.audio_device = config.captureAudio ? audioDevUtf8.constData() : nullptr;

    ffiConfig.capture_microphone = config.captureMicrophone;

    QByteArray micDevUtf8 = config.microphoneDevice.toUtf8();
    ffiConfig.microphone_device = config.captureMicrophone ? micDevUtf8.constData() : nullptr;

    SDError* error = nullptr;
    SDRecordingHandle* handle = sd_start_recording(ffiConfig, &error);
    checkError(error, "sd_start_recording");

    if (!handle) {
        throw std::runtime_error("sd_start_recording returned null handle");
    }

    return handle;
}

QString RustBridge::stopRecording(SDRecordingHandle* handle) {
    char* outPath = nullptr;
    SDError* error = nullptr;

    sd_stop_recording(handle, &outPath, &error);
    checkError(error, "sd_stop_recording");

    QString result;
    if (outPath) {
        result = QString::fromUtf8(outPath);
        sd_free_string(outPath);
    }
    return result;
}

void RustBridge::pauseRecording(SDRecordingHandle* handle) {
    SDError* error = nullptr;
    sd_pause_recording(handle, &error);
    checkError(error, "sd_pause_recording");
}

void RustBridge::resumeRecording(SDRecordingHandle* handle) {
    SDError* error = nullptr;
    sd_resume_recording(handle, &error);
    checkError(error, "sd_resume_recording");
}

RecordingStatus RustBridge::getRecordingStatus(SDRecordingHandle* handle) {
    SDRecordingStatus raw = sd_get_recording_status(handle);
    RecordingStatus status;
    status.isRecording = raw.is_recording;
    status.isPaused = raw.is_paused;
    status.durationSecs = raw.duration_secs;
    status.bytesWritten = raw.bytes_written;
    return status;
}

// ── Settings ────────────────────────────────────────────────────────────────

QJsonObject RustBridge::loadSettings() {
    SDError* error = nullptr;
    char* json = sd_load_settings(&error);
    checkError(error, "sd_load_settings");

    QJsonObject result;
    if (json) {
        QJsonDocument doc = QJsonDocument::fromJson(QByteArray(json));
        if (doc.isObject()) {
            result = doc.object();
        }
        sd_free_string(json);
    }
    return result;
}

void RustBridge::saveSettings(const QJsonObject& settings) {
    QJsonDocument doc(settings);
    QByteArray jsonUtf8 = doc.toJson(QJsonDocument::Compact);
    SDError* error = nullptr;

    sd_save_settings(jsonUtf8.constData(), &error);
    checkError(error, "sd_save_settings");
}

// ── FFmpeg ──────────────────────────────────────────────────────────────────

FfmpegStatus RustBridge::getFfmpegStatus() {
    SDError* error = nullptr;
    SDFfmpegStatus* raw = sd_get_ffmpeg_status(&error);
    checkError(error, "sd_get_ffmpeg_status");

    FfmpegStatus status;
    if (raw) {
        status.available = raw->available;
        status.version = QString::fromUtf8(raw->version ? raw->version : "");
        status.path = QString::fromUtf8(raw->path ? raw->path : "");

        for (size_t i = 0; i < raw->encoder_count; ++i) {
            status.encoders.append(QString::fromUtf8(raw->encoders[i]));
        }

        sd_free_ffmpeg_status(raw);
    }
    return status;
}

// ── Audio Devices ───────────────────────────────────────────────────────────

QVector<AudioDeviceInfo> RustBridge::listAudioDevices() {
    SDError* error = nullptr;
    char* json = sd_list_audio_devices(&error);
    checkError(error, "sd_list_audio_devices");

    QVector<AudioDeviceInfo> devices;
    if (json) {
        QJsonDocument doc = QJsonDocument::fromJson(QByteArray(json));
        if (doc.isArray()) {
            for (const QJsonValue& val : doc.array()) {
                QJsonObject obj = val.toObject();
                AudioDeviceInfo dev;
                dev.id = obj["id"].toString();
                dev.name = obj["name"].toString();
                dev.isDefault = obj["is_default"].toBool();
                dev.isInput = obj["is_input"].toBool();
                devices.append(dev);
            }
        }
        sd_free_string(json);
    }
    return devices;
}

} // namespace sd
```

---

### Task 11: AppState — Singleton Application State

**Files:** `qt-ui/src/core/AppState.h`, `qt-ui/src/core/AppState.cpp`

#### Steps

- [ ] **11.1** Create `AppState.h` with singleton accessor, RustBridge ownership, and Qt signals
- [ ] **11.2** Create `AppState.cpp` implementing initialization and state management
- [ ] **11.3** Verify signals compile with AUTOMOC

#### AppState.h

```cpp
// qt-ui/src/core/AppState.h
#pragma once

#include "core/RustBridge.h"

#include <QObject>
#include <QString>
#include <QJsonObject>
#include <memory>

namespace sd {

class AppState : public QObject {
    Q_OBJECT

public:
    /// Returns the singleton instance. Must call initialize() before first use.
    static AppState* instance();

    /// Creates the singleton and initializes the RustBridge.
    /// Throws std::runtime_error if Rust init fails.
    static void initialize(const QString& configDir = QString());

    /// Destroys the singleton. Call at shutdown.
    static void destroy();

    /// Access the RustBridge (never null after initialize()).
    RustBridge* bridge() const { return m_bridge.get(); }

    // ── Recording state ─────────────────────────────────────────────────

    bool isRecording() const { return m_recording; }
    bool isPaused() const { return m_paused; }
    SDRecordingHandle* recordingHandle() const { return m_recordingHandle; }

    void setRecording(bool recording);
    void setPaused(bool paused);
    void setRecordingHandle(SDRecordingHandle* handle);

    // ── Settings cache ──────────────────────────────────────────────────

    QJsonObject settings() const { return m_settings; }
    void reloadSettings();
    void commitSettings(const QJsonObject& settings);

    // ── Output directory ────────────────────────────────────────────────

    QString outputDirectory() const;

signals:
    void recordingStateChanged(bool recording);
    void pauseStateChanged(bool paused);
    void settingsChanged(const QJsonObject& settings);
    void sourcesRefreshed();
    void errorOccurred(const QString& message);

private:
    explicit AppState(const QString& configDir, QObject* parent = nullptr);
    ~AppState() override;

    static AppState* s_instance;

    std::unique_ptr<RustBridge> m_bridge;
    bool m_recording = false;
    bool m_paused = false;
    SDRecordingHandle* m_recordingHandle = nullptr;
    QJsonObject m_settings;
};

} // namespace sd
```

#### AppState.cpp

```cpp
// qt-ui/src/core/AppState.cpp
#include "core/AppState.h"

#include <QStandardPaths>
#include <QDir>
#include <QDebug>

namespace sd {

AppState* AppState::s_instance = nullptr;

AppState* AppState::instance() {
    Q_ASSERT(s_instance != nullptr);
    return s_instance;
}

void AppState::initialize(const QString& configDir) {
    if (s_instance) {
        qWarning() << "AppState already initialized";
        return;
    }
    s_instance = new AppState(configDir);
}

void AppState::destroy() {
    delete s_instance;
    s_instance = nullptr;
}

AppState::AppState(const QString& configDir, QObject* parent)
    : QObject(parent)
{
    m_bridge = std::make_unique<RustBridge>(configDir);
    reloadSettings();
}

AppState::~AppState() {
    // If a recording is in progress, stop it gracefully
    if (m_recording && m_recordingHandle) {
        try {
            m_bridge->stopRecording(m_recordingHandle);
        } catch (const std::exception& e) {
            qWarning() << "Failed to stop recording during shutdown:" << e.what();
        }
    }
}

void AppState::setRecording(bool recording) {
    if (m_recording != recording) {
        m_recording = recording;
        if (!recording) {
            m_paused = false;
            m_recordingHandle = nullptr;
        }
        emit recordingStateChanged(m_recording);
    }
}

void AppState::setPaused(bool paused) {
    if (m_paused != paused) {
        m_paused = paused;
        emit pauseStateChanged(m_paused);
    }
}

void AppState::setRecordingHandle(SDRecordingHandle* handle) {
    m_recordingHandle = handle;
}

void AppState::reloadSettings() {
    try {
        m_settings = m_bridge->loadSettings();
        emit settingsChanged(m_settings);
    } catch (const std::exception& e) {
        qWarning() << "Failed to load settings:" << e.what();
        m_settings = QJsonObject();
    }
}

void AppState::commitSettings(const QJsonObject& settings) {
    try {
        m_bridge->saveSettings(settings);
        m_settings = settings;
        emit settingsChanged(m_settings);
    } catch (const std::exception& e) {
        emit errorOccurred(
            QString("Failed to save settings: %1").arg(e.what()));
    }
}

QString AppState::outputDirectory() const {
    QString dir = m_settings.value("output_directory").toString();
    if (dir.isEmpty()) {
        dir = QStandardPaths::writableLocation(QStandardPaths::MoviesLocation)
              + "/ScreenDream";
    }
    QDir().mkpath(dir);
    return dir;
}

} // namespace sd
```

---

## Phase 4: Qt6 Project Setup & Core Windows

---

### Task 12: main.cpp — Application Entry Point

**File:** `qt-ui/src/main.cpp`

#### Steps

- [ ] **12.1** Create `main.cpp` with `QApplication` setup
- [ ] **12.2** Set application metadata (name, version, organization)
- [ ] **12.3** Load dark theme QSS from resources
- [ ] **12.4** Initialize AppState and RustBridge
- [ ] **12.5** Create and show MainWindow
- [ ] **12.6** Set up system tray icon with context menu
- [ ] **12.7** Execute event loop

#### Complete File

```cpp
// qt-ui/src/main.cpp
#include <QApplication>
#include <QFile>
#include <QSystemTrayIcon>
#include <QMenu>
#include <QAction>
#include <QMessageBox>
#include <QIcon>
#include <QDebug>

#include "core/AppState.h"
#include "ui/MainWindow.h"

static void loadStylesheet(QApplication& app) {
    QFile styleFile(":/styles/dark-theme.qss");
    if (styleFile.open(QFile::ReadOnly | QFile::Text)) {
        app.setStyleSheet(QString::fromUtf8(styleFile.readAll()));
        styleFile.close();
    } else {
        qWarning() << "Failed to load dark theme stylesheet";
    }
}

static QSystemTrayIcon* createTrayIcon(sd::MainWindow* mainWindow) {
    auto* trayIcon = new QSystemTrayIcon(mainWindow);
    trayIcon->setIcon(QIcon::fromTheme("camera-video",
        QIcon(":/icons/app-icon.png")));
    trayIcon->setToolTip("Screen Dream");

    auto* trayMenu = new QMenu();

    auto* showAction = trayMenu->addAction("Show Window");
    QObject::connect(showAction, &QAction::triggered, mainWindow, [mainWindow]() {
        mainWindow->show();
        mainWindow->raise();
        mainWindow->activateWindow();
    });

    auto* screenshotAction = trayMenu->addAction("Quick Screenshot");
    QObject::connect(screenshotAction, &QAction::triggered, mainWindow,
        &sd::MainWindow::onQuickScreenshot);

    trayMenu->addSeparator();

    auto* quitAction = trayMenu->addAction("Quit");
    QObject::connect(quitAction, &QAction::triggered,
        QApplication::instance(), &QApplication::quit);

    trayIcon->setContextMenu(trayMenu);

    QObject::connect(trayIcon, &QSystemTrayIcon::activated,
        mainWindow, [mainWindow](QSystemTrayIcon::ActivationReason reason) {
            if (reason == QSystemTrayIcon::DoubleClick) {
                mainWindow->show();
                mainWindow->raise();
                mainWindow->activateWindow();
            }
        });

    return trayIcon;
}

int main(int argc, char* argv[]) {
    QApplication app(argc, argv);
    app.setApplicationName("Screen Dream");
    app.setApplicationVersion("0.1.0");
    app.setOrganizationName("ScreenDream");
    app.setOrganizationDomain("screendream.app");

    // Load dark theme
    loadStylesheet(app);

    // Initialize Rust backend
    try {
        sd::AppState::initialize();
    } catch (const std::exception& e) {
        QMessageBox::critical(nullptr, "Initialization Error",
            QString("Failed to initialize Screen Dream backend:\n\n%1")
                .arg(e.what()));
        return 1;
    }

    // Create main window
    auto* mainWindow = new sd::MainWindow();
    mainWindow->show();

    // System tray
    auto* trayIcon = createTrayIcon(mainWindow);
    trayIcon->show();

    int result = app.exec();

    // Cleanup
    sd::AppState::destroy();
    return result;
}
```

---

### Task 13: Dark Theme QSS (Complete Stylesheet)

**File:** `qt-ui/src/resources/styles/dark-theme.qss`

#### Steps

- [ ] **13.1** Create `resources/styles/` directory
- [ ] **13.2** Write complete dark-theme.qss with all widget styles
- [ ] **13.3** Create `resources.qrc` referencing the stylesheet and icons
- [ ] **13.4** Verify stylesheet loads at runtime

#### resources.qrc

```xml
<!-- qt-ui/src/resources/resources.qrc -->
<RCC>
    <qresource prefix="/">
        <file>styles/dark-theme.qss</file>
        <file>icons/app-icon.png</file>
        <file>icons/screen.png</file>
        <file>icons/window.png</file>
        <file>icons/region.png</file>
        <file>icons/screenshot.png</file>
        <file>icons/video.png</file>
    </qresource>
</RCC>
```

#### dark-theme.qss

```css
/* qt-ui/src/resources/styles/dark-theme.qss
 * Screen Dream — Dark Theme
 *
 * Palette:
 *   background:  #1a1a2e
 *   panels:      #16213e
 *   surface:     #1f2b47
 *   accent:      #e94560
 *   accent-hover:#ff6b81
 *   text:        #e0e0e0
 *   text-dim:    #8892a4
 *   border:      #2a3a5c
 *   focus:       #e94560
 */

/* ── Global ─────────────────────────────────────────────────────────────── */

* {
    font-family: "Inter", "Segoe UI", "Helvetica Neue", Arial, sans-serif;
    font-size: 14px;
    color: #e0e0e0;
}

/* ── QMainWindow ────────────────────────────────────────────────────────── */

QMainWindow {
    background-color: #1a1a2e;
}

QMainWindow::separator {
    background-color: #2a3a5c;
    width: 1px;
    height: 1px;
}

/* ── QWidget ────────────────────────────────────────────────────────────── */

QWidget {
    background-color: #1a1a2e;
    color: #e0e0e0;
}

/* ── QPushButton ────────────────────────────────────────────────────────── */

QPushButton {
    background-color: #16213e;
    color: #e0e0e0;
    border: 1px solid #2a3a5c;
    border-radius: 6px;
    padding: 8px 18px;
    min-height: 20px;
}

QPushButton:hover {
    background-color: #1f2b47;
    border-color: #e94560;
}

QPushButton:pressed {
    background-color: #e94560;
    color: #ffffff;
    border-color: #e94560;
}

QPushButton:focus {
    outline: none;
    border-color: #e94560;
    border-width: 2px;
}

QPushButton:disabled {
    background-color: #12192e;
    color: #555b6e;
    border-color: #1e2a44;
}

QPushButton#accentButton {
    background-color: #e94560;
    color: #ffffff;
    border: none;
    font-weight: bold;
}

QPushButton#accentButton:hover {
    background-color: #ff6b81;
}

QPushButton#accentButton:pressed {
    background-color: #c73a52;
}

/* ── QLineEdit ──────────────────────────────────────────────────────────── */

QLineEdit {
    background-color: #16213e;
    color: #e0e0e0;
    border: 1px solid #2a3a5c;
    border-radius: 4px;
    padding: 6px 10px;
    selection-background-color: #e94560;
    selection-color: #ffffff;
}

QLineEdit:focus {
    border-color: #e94560;
    border-width: 2px;
}

QLineEdit:disabled {
    background-color: #12192e;
    color: #555b6e;
}

/* ── QComboBox ──────────────────────────────────────────────────────────── */

QComboBox {
    background-color: #16213e;
    color: #e0e0e0;
    border: 1px solid #2a3a5c;
    border-radius: 4px;
    padding: 6px 10px;
    min-width: 100px;
}

QComboBox:hover {
    border-color: #e94560;
}

QComboBox:focus {
    border-color: #e94560;
    border-width: 2px;
}

QComboBox::drop-down {
    border: none;
    width: 30px;
}

QComboBox::down-arrow {
    image: none;
    border-left: 5px solid transparent;
    border-right: 5px solid transparent;
    border-top: 6px solid #8892a4;
    margin-right: 10px;
}

QComboBox QAbstractItemView {
    background-color: #16213e;
    color: #e0e0e0;
    border: 1px solid #2a3a5c;
    selection-background-color: #e94560;
    selection-color: #ffffff;
    outline: none;
}

/* ── QLabel ─────────────────────────────────────────────────────────────── */

QLabel {
    background-color: transparent;
    color: #e0e0e0;
}

QLabel#dimLabel {
    color: #8892a4;
}

QLabel#titleLabel {
    font-size: 18px;
    font-weight: bold;
}

/* ── QGroupBox ──────────────────────────────────────────────────────────── */

QGroupBox {
    background-color: #16213e;
    border: 1px solid #2a3a5c;
    border-radius: 6px;
    margin-top: 14px;
    padding: 16px 12px 12px 12px;
    font-weight: bold;
}

QGroupBox::title {
    subcontrol-origin: margin;
    subcontrol-position: top left;
    padding: 2px 8px;
    color: #e94560;
}

/* ── QTabWidget ─────────────────────────────────────────────────────────── */

QTabWidget::pane {
    background-color: #16213e;
    border: 1px solid #2a3a5c;
    border-radius: 0 0 6px 6px;
}

QTabBar::tab {
    background-color: #1a1a2e;
    color: #8892a4;
    border: 1px solid #2a3a5c;
    border-bottom: none;
    padding: 8px 20px;
    margin-right: 2px;
    border-radius: 4px 4px 0 0;
}

QTabBar::tab:selected {
    background-color: #16213e;
    color: #e0e0e0;
    border-bottom: 2px solid #e94560;
}

QTabBar::tab:hover:!selected {
    background-color: #1f2b47;
    color: #e0e0e0;
}

/* ── QListWidget ────────────────────────────────────────────────────────── */

QListWidget {
    background-color: #16213e;
    border: 1px solid #2a3a5c;
    border-radius: 4px;
    outline: none;
}

QListWidget::item {
    padding: 6px 10px;
    border-bottom: 1px solid #1e2a44;
}

QListWidget::item:selected {
    background-color: #e94560;
    color: #ffffff;
}

QListWidget::item:hover:!selected {
    background-color: #1f2b47;
}

/* ── QScrollArea ────────────────────────────────────────────────────────── */

QScrollArea {
    background-color: #1a1a2e;
    border: none;
}

/* ── QScrollBar (Vertical) ──────────────────────────────────────────────── */

QScrollBar:vertical {
    background-color: #1a1a2e;
    width: 10px;
    margin: 0;
    border-radius: 5px;
}

QScrollBar::handle:vertical {
    background-color: #2a3a5c;
    min-height: 30px;
    border-radius: 5px;
}

QScrollBar::handle:vertical:hover {
    background-color: #3a4d70;
}

QScrollBar::add-line:vertical,
QScrollBar::sub-line:vertical {
    height: 0;
    background: none;
}

QScrollBar::add-page:vertical,
QScrollBar::sub-page:vertical {
    background: none;
}

/* ── QScrollBar (Horizontal) ────────────────────────────────────────────── */

QScrollBar:horizontal {
    background-color: #1a1a2e;
    height: 10px;
    margin: 0;
    border-radius: 5px;
}

QScrollBar::handle:horizontal {
    background-color: #2a3a5c;
    min-width: 30px;
    border-radius: 5px;
}

QScrollBar::handle:horizontal:hover {
    background-color: #3a4d70;
}

QScrollBar::add-line:horizontal,
QScrollBar::sub-line:horizontal {
    width: 0;
    background: none;
}

QScrollBar::add-page:horizontal,
QScrollBar::sub-page:horizontal {
    background: none;
}

/* ── QSlider ────────────────────────────────────────────────────────────── */

QSlider::groove:horizontal {
    background-color: #2a3a5c;
    height: 6px;
    border-radius: 3px;
}

QSlider::handle:horizontal {
    background-color: #e94560;
    width: 16px;
    height: 16px;
    margin: -5px 0;
    border-radius: 8px;
}

QSlider::handle:horizontal:hover {
    background-color: #ff6b81;
}

QSlider::sub-page:horizontal {
    background-color: #e94560;
    border-radius: 3px;
}

/* ── QProgressBar ───────────────────────────────────────────────────────── */

QProgressBar {
    background-color: #16213e;
    border: 1px solid #2a3a5c;
    border-radius: 4px;
    text-align: center;
    color: #e0e0e0;
    min-height: 18px;
}

QProgressBar::chunk {
    background-color: #e94560;
    border-radius: 3px;
}

/* ── QMenuBar ───────────────────────────────────────────────────────────── */

QMenuBar {
    background-color: #16213e;
    color: #e0e0e0;
    border-bottom: 1px solid #2a3a5c;
    padding: 2px 0;
}

QMenuBar::item {
    background-color: transparent;
    padding: 6px 12px;
}

QMenuBar::item:selected {
    background-color: #1f2b47;
    border-radius: 4px;
}

QMenuBar::item:pressed {
    background-color: #e94560;
    color: #ffffff;
}

/* ── QMenu ──────────────────────────────────────────────────────────────── */

QMenu {
    background-color: #16213e;
    color: #e0e0e0;
    border: 1px solid #2a3a5c;
    border-radius: 6px;
    padding: 4px 0;
}

QMenu::item {
    padding: 8px 30px 8px 20px;
}

QMenu::item:selected {
    background-color: #e94560;
    color: #ffffff;
}

QMenu::separator {
    height: 1px;
    background-color: #2a3a5c;
    margin: 4px 10px;
}

QMenu::indicator {
    width: 16px;
    height: 16px;
    margin-left: 6px;
}

/* ── QStatusBar ─────────────────────────────────────────────────────────── */

QStatusBar {
    background-color: #16213e;
    color: #8892a4;
    border-top: 1px solid #2a3a5c;
    font-size: 12px;
}

QStatusBar::item {
    border: none;
}

/* ── QToolBar ───────────────────────────────────────────────────────────── */

QToolBar {
    background-color: #16213e;
    border-bottom: 1px solid #2a3a5c;
    padding: 4px;
    spacing: 4px;
}

QToolBar::separator {
    width: 1px;
    background-color: #2a3a5c;
    margin: 4px 6px;
}

QToolButton {
    background-color: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
    padding: 6px;
}

QToolButton:hover {
    background-color: #1f2b47;
    border-color: #2a3a5c;
}

QToolButton:pressed {
    background-color: #e94560;
}

/* ── QSplitter ──────────────────────────────────────────────────────────── */

QSplitter::handle {
    background-color: #2a3a5c;
}

QSplitter::handle:horizontal {
    width: 2px;
}

QSplitter::handle:vertical {
    height: 2px;
}

QSplitter::handle:hover {
    background-color: #e94560;
}

/* ── QHeaderView ────────────────────────────────────────────────────────── */

QHeaderView {
    background-color: #16213e;
}

QHeaderView::section {
    background-color: #16213e;
    color: #8892a4;
    border: none;
    border-right: 1px solid #2a3a5c;
    border-bottom: 1px solid #2a3a5c;
    padding: 6px 10px;
    font-weight: bold;
}

QHeaderView::section:hover {
    background-color: #1f2b47;
    color: #e0e0e0;
}

/* ── QTableView ─────────────────────────────────────────────────────────── */

QTableView {
    background-color: #1a1a2e;
    alternate-background-color: #16213e;
    border: 1px solid #2a3a5c;
    gridline-color: #2a3a5c;
    selection-background-color: #e94560;
    selection-color: #ffffff;
    outline: none;
}

/* ── QTreeView ──────────────────────────────────────────────────────────── */

QTreeView {
    background-color: #1a1a2e;
    alternate-background-color: #16213e;
    border: 1px solid #2a3a5c;
    selection-background-color: #e94560;
    selection-color: #ffffff;
    outline: none;
}

QTreeView::item {
    padding: 4px;
}

QTreeView::item:hover {
    background-color: #1f2b47;
}

QTreeView::branch:has-children:!has-siblings:closed,
QTreeView::branch:closed:has-children:has-siblings {
    border-image: none;
}

QTreeView::branch:open:has-children:!has-siblings,
QTreeView::branch:open:has-children:has-siblings {
    border-image: none;
}

/* ── QDialog ────────────────────────────────────────────────────────────── */

QDialog {
    background-color: #1a1a2e;
}

/* ── QDialogButtonBox ───────────────────────────────────────────────────── */

QDialogButtonBox {
    button-layout: 0;
}

/* ── QCheckBox ──────────────────────────────────────────────────────────── */

QCheckBox {
    spacing: 8px;
    color: #e0e0e0;
    background-color: transparent;
}

QCheckBox::indicator {
    width: 18px;
    height: 18px;
    border: 2px solid #2a3a5c;
    border-radius: 3px;
    background-color: #16213e;
}

QCheckBox::indicator:checked {
    background-color: #e94560;
    border-color: #e94560;
}

QCheckBox::indicator:hover {
    border-color: #e94560;
}

QCheckBox::indicator:disabled {
    background-color: #12192e;
    border-color: #1e2a44;
}

/* ── QRadioButton ───────────────────────────────────────────────────────── */

QRadioButton {
    spacing: 8px;
    color: #e0e0e0;
    background-color: transparent;
}

QRadioButton::indicator {
    width: 18px;
    height: 18px;
    border: 2px solid #2a3a5c;
    border-radius: 10px;
    background-color: #16213e;
}

QRadioButton::indicator:checked {
    background-color: #e94560;
    border-color: #e94560;
}

QRadioButton::indicator:hover {
    border-color: #e94560;
}

/* ── QSpinBox ───────────────────────────────────────────────────────────── */

QSpinBox, QDoubleSpinBox {
    background-color: #16213e;
    color: #e0e0e0;
    border: 1px solid #2a3a5c;
    border-radius: 4px;
    padding: 4px 8px;
}

QSpinBox:focus, QDoubleSpinBox:focus {
    border-color: #e94560;
}

QSpinBox::up-button, QDoubleSpinBox::up-button {
    background-color: #1f2b47;
    border-left: 1px solid #2a3a5c;
    border-bottom: 1px solid #2a3a5c;
    width: 20px;
}

QSpinBox::down-button, QDoubleSpinBox::down-button {
    background-color: #1f2b47;
    border-left: 1px solid #2a3a5c;
    width: 20px;
}

QSpinBox::up-button:hover, QDoubleSpinBox::up-button:hover,
QSpinBox::down-button:hover, QDoubleSpinBox::down-button:hover {
    background-color: #2a3a5c;
}

/* ── QToolTip ───────────────────────────────────────────────────────────── */

QToolTip {
    background-color: #16213e;
    color: #e0e0e0;
    border: 1px solid #2a3a5c;
    border-radius: 4px;
    padding: 4px 8px;
    font-size: 12px;
}

/* ── Focus Indicator (global) ───────────────────────────────────────────── */

*:focus {
    outline: none;
}
```

---

### Task 14: MainWindow Shell

**Files:** `qt-ui/src/ui/MainWindow.h`, `qt-ui/src/ui/MainWindow.cpp`

#### Steps

- [ ] **14.1** Create `MainWindow.h` with class definition, menu bar, status bar, and capture card layout
- [ ] **14.2** Implement `MainWindow.cpp` with full layout construction
- [ ] **14.3** Wire CaptureCard signals to slots
- [ ] **14.4** Populate status bar with FFmpeg version and platform info
- [ ] **14.5** Add menu bar with File, Edit, View, Help menus

#### MainWindow.h

```cpp
// qt-ui/src/ui/MainWindow.h
#pragma once

#include <QMainWindow>
#include <QLabel>
#include <QPushButton>
#include <QHBoxLayout>
#include <QVBoxLayout>

namespace sd {

class CaptureCard;
class RecentCaptures;

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(QWidget* parent = nullptr);
    ~MainWindow() override;

public slots:
    void onQuickScreenshot();

private slots:
    void onScreenScreenshot();
    void onScreenVideo();
    void onWindowScreenshot();
    void onWindowVideo();
    void onAreaScreenshot();
    void onAreaVideo();
    void onOpenSettings();
    void onAbout();

private:
    void setupMenuBar();
    void setupCentralWidget();
    void setupStatusBar();
    void refreshStatusBar();

    // Capture cards
    CaptureCard* m_screenCard = nullptr;
    CaptureCard* m_windowCard = nullptr;
    CaptureCard* m_areaCard = nullptr;

    // Recent captures
    RecentCaptures* m_recentCaptures = nullptr;

    // Status bar widgets
    QLabel* m_ffmpegStatusLabel = nullptr;
    QLabel* m_platformInfoLabel = nullptr;
    QPushButton* m_settingsButton = nullptr;
};

} // namespace sd
```

#### MainWindow.cpp

```cpp
// qt-ui/src/ui/MainWindow.cpp
#include "ui/MainWindow.h"
#include "widgets/CaptureCard.h"
#include "widgets/RecentCaptures.h"
#include "core/AppState.h"

#include <QMenuBar>
#include <QStatusBar>
#include <QMessageBox>
#include <QFileDialog>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QFrame>
#include <QAction>
#include <QDebug>

namespace sd {

MainWindow::MainWindow(QWidget* parent)
    : QMainWindow(parent)
{
    setWindowTitle("Screen Dream");
    setMinimumSize(800, 550);
    resize(960, 640);

    setupMenuBar();
    setupCentralWidget();
    setupStatusBar();
    refreshStatusBar();
}

MainWindow::~MainWindow() = default;

// ── Menu Bar ────────────────────────────────────────────────────────────────

void MainWindow::setupMenuBar() {
    auto* menuBar = this->menuBar();

    // File menu
    auto* fileMenu = menuBar->addMenu("&File");

    auto* screenshotAction = fileMenu->addAction("&Quick Screenshot");
    screenshotAction->setShortcut(QKeySequence("Ctrl+Shift+S"));
    connect(screenshotAction, &QAction::triggered, this, &MainWindow::onQuickScreenshot);

    fileMenu->addSeparator();

    auto* openOutputDir = fileMenu->addAction("&Open Output Folder...");
    connect(openOutputDir, &QAction::triggered, this, []() {
        QString dir = AppState::instance()->outputDirectory();
        QDesktopServices::openUrl(QUrl::fromLocalFile(dir));
    });

    fileMenu->addSeparator();

    auto* quitAction = fileMenu->addAction("&Quit");
    quitAction->setShortcut(QKeySequence::Quit);
    connect(quitAction, &QAction::triggered, this, &QMainWindow::close);

    // Edit menu
    auto* editMenu = menuBar->addMenu("&Edit");

    auto* settingsAction = editMenu->addAction("&Settings...");
    settingsAction->setShortcut(QKeySequence("Ctrl+,"));
    connect(settingsAction, &QAction::triggered, this, &MainWindow::onOpenSettings);

    // View menu
    auto* viewMenu = menuBar->addMenu("&View");

    auto* refreshAction = viewMenu->addAction("&Refresh Sources");
    refreshAction->setShortcut(QKeySequence("F5"));
    connect(refreshAction, &QAction::triggered, this, [this]() {
        m_recentCaptures->refresh();
        refreshStatusBar();
    });

    // Help menu
    auto* helpMenu = menuBar->addMenu("&Help");

    auto* aboutAction = helpMenu->addAction("&About Screen Dream");
    connect(aboutAction, &QAction::triggered, this, &MainWindow::onAbout);
}

// ── Central Widget ──────────────────────────────────────────────────────────

void MainWindow::setupCentralWidget() {
    auto* centralWidget = new QWidget(this);
    auto* mainLayout = new QVBoxLayout(centralWidget);
    mainLayout->setContentsMargins(24, 24, 24, 16);
    mainLayout->setSpacing(24);

    // Title
    auto* titleLabel = new QLabel("Screen Dream");
    titleLabel->setObjectName("titleLabel");
    titleLabel->setAlignment(Qt::AlignCenter);
    mainLayout->addWidget(titleLabel);

    // Capture cards row
    auto* cardsLayout = new QHBoxLayout();
    cardsLayout->setSpacing(16);

    m_screenCard = new CaptureCard("Full Screen", "Capture your entire screen",
                                    ":/icons/screen.png", centralWidget);
    m_windowCard = new CaptureCard("Window", "Capture a specific window",
                                    ":/icons/window.png", centralWidget);
    m_areaCard = new CaptureCard("Area", "Select a region to capture",
                                  ":/icons/region.png", centralWidget);

    cardsLayout->addWidget(m_screenCard);
    cardsLayout->addWidget(m_windowCard);
    cardsLayout->addWidget(m_areaCard);

    mainLayout->addLayout(cardsLayout);

    // Wire card signals
    connect(m_screenCard, &CaptureCard::screenshotClicked, this, &MainWindow::onScreenScreenshot);
    connect(m_screenCard, &CaptureCard::videoClicked, this, &MainWindow::onScreenVideo);
    connect(m_windowCard, &CaptureCard::screenshotClicked, this, &MainWindow::onWindowScreenshot);
    connect(m_windowCard, &CaptureCard::videoClicked, this, &MainWindow::onWindowVideo);
    connect(m_areaCard, &CaptureCard::screenshotClicked, this, &MainWindow::onAreaScreenshot);
    connect(m_areaCard, &CaptureCard::videoClicked, this, &MainWindow::onAreaVideo);

    // Separator
    auto* separator = new QFrame(centralWidget);
    separator->setFrameShape(QFrame::HLine);
    separator->setStyleSheet("background-color: #2a3a5c; max-height: 1px;");
    mainLayout->addWidget(separator);

    // Recent captures
    auto* recentLabel = new QLabel("Recent Captures");
    recentLabel->setStyleSheet("font-size: 16px; font-weight: bold; color: #8892a4;");
    mainLayout->addWidget(recentLabel);

    m_recentCaptures = new RecentCaptures(centralWidget);
    mainLayout->addWidget(m_recentCaptures, 1);  // stretch factor 1

    setCentralWidget(centralWidget);
}

// ── Status Bar ──────────────────────────────────────────────────────────────

void MainWindow::setupStatusBar() {
    auto* bar = statusBar();

    m_ffmpegStatusLabel = new QLabel(bar);
    m_platformInfoLabel = new QLabel(bar);
    m_settingsButton = new QPushButton("Settings", bar);
    m_settingsButton->setFlat(true);
    m_settingsButton->setCursor(Qt::PointingHandCursor);
    m_settingsButton->setStyleSheet(
        "QPushButton { color: #8892a4; border: none; padding: 2px 8px; }"
        "QPushButton:hover { color: #e94560; }"
    );
    connect(m_settingsButton, &QPushButton::clicked, this, &MainWindow::onOpenSettings);

    bar->addWidget(m_ffmpegStatusLabel);
    bar->addWidget(m_platformInfoLabel, 1);
    bar->addPermanentWidget(m_settingsButton);
}

void MainWindow::refreshStatusBar() {
    auto* bridge = AppState::instance()->bridge();

    try {
        auto ffmpeg = bridge->getFfmpegStatus();
        if (ffmpeg.available) {
            m_ffmpegStatusLabel->setText(
                QString("FFmpeg %1").arg(ffmpeg.version));
            m_ffmpegStatusLabel->setStyleSheet("color: #4ade80;"); // green
        } else {
            m_ffmpegStatusLabel->setText("FFmpeg: not found");
            m_ffmpegStatusLabel->setStyleSheet("color: #e94560;"); // red
        }
    } catch (...) {
        m_ffmpegStatusLabel->setText("FFmpeg: error");
        m_ffmpegStatusLabel->setStyleSheet("color: #e94560;");
    }

    try {
        auto platform = bridge->getPlatformInfo();
        m_platformInfoLabel->setText(
            QString("%1 | %2").arg(platform.os, platform.displayServer));
    } catch (...) {
        m_platformInfoLabel->setText("");
    }
}

// ── Slots ───────────────────────────────────────────────────────────────────

void MainWindow::onQuickScreenshot() {
    // Full-screen screenshot with default settings
    auto* bridge = AppState::instance()->bridge();
    try {
        CaptureSource source;
        source.type = CaptureSource::FullScreen;
        source.sourceId = 0;

        QString outputDir = AppState::instance()->outputDirectory();
        QString filename = outputDir + "/screenshot_" +
            QDateTime::currentDateTime().toString("yyyyMMdd_HHmmss") + ".png";

        bridge->takeScreenshot(source, filename);
        statusBar()->showMessage(QString("Screenshot saved: %1").arg(filename), 5000);
        m_recentCaptures->refresh();
    } catch (const std::exception& e) {
        statusBar()->showMessage(QString("Screenshot failed: %1").arg(e.what()), 5000);
    }
}

void MainWindow::onScreenScreenshot() {
    onQuickScreenshot();
}

void MainWindow::onScreenVideo() {
    // TODO: Implement in Phase 5 (RecordingEngine integration)
    statusBar()->showMessage("Screen recording — coming soon", 3000);
}

void MainWindow::onWindowScreenshot() {
    // TODO: Open SourcePickerDialog for window selection
    statusBar()->showMessage("Window screenshot — coming soon", 3000);
}

void MainWindow::onWindowVideo() {
    statusBar()->showMessage("Window recording — coming soon", 3000);
}

void MainWindow::onAreaScreenshot() {
    // TODO: Open RegionSelector overlay
    statusBar()->showMessage("Area screenshot — coming soon", 3000);
}

void MainWindow::onAreaVideo() {
    statusBar()->showMessage("Area recording — coming soon", 3000);
}

void MainWindow::onOpenSettings() {
    // TODO: Open SettingsDialog (Phase 5)
    statusBar()->showMessage("Settings dialog — coming soon", 3000);
}

void MainWindow::onAbout() {
    QMessageBox::about(this, "About Screen Dream",
        "<h3>Screen Dream</h3>"
        "<p>Version 0.1.0</p>"
        "<p>A powerful screen capture and recording application.</p>"
        "<p>License: GPLv3</p>");
}

} // namespace sd
```

---

### Task 15: CaptureCard Widget

**Files:** `qt-ui/src/widgets/CaptureCard.h`, `qt-ui/src/widgets/CaptureCard.cpp`

#### Steps

- [ ] **15.1** Create `CaptureCard.h` with properties, signals, and hover support
- [ ] **15.2** Implement `CaptureCard.cpp` with icon, title, description, and two action buttons
- [ ] **15.3** Implement hover effect via enterEvent/leaveEvent
- [ ] **15.4** Verify layout and signals

#### CaptureCard.h

```cpp
// qt-ui/src/widgets/CaptureCard.h
#pragma once

#include <QWidget>
#include <QLabel>
#include <QPushButton>
#include <QString>

namespace sd {

class CaptureCard : public QWidget {
    Q_OBJECT

public:
    explicit CaptureCard(const QString& title,
                         const QString& description,
                         const QString& iconPath,
                         QWidget* parent = nullptr);
    ~CaptureCard() override;

signals:
    void screenshotClicked();
    void videoClicked();

protected:
    void enterEvent(QEnterEvent* event) override;
    void leaveEvent(QEvent* event) override;
    void paintEvent(QPaintEvent* event) override;

private:
    void setupUi();
    void updateStyle(bool hovered);

    QString m_title;
    QString m_description;
    QString m_iconPath;
    bool m_hovered = false;

    QLabel* m_iconLabel = nullptr;
    QLabel* m_titleLabel = nullptr;
    QLabel* m_descLabel = nullptr;
    QPushButton* m_screenshotBtn = nullptr;
    QPushButton* m_videoBtn = nullptr;
};

} // namespace sd
```

#### CaptureCard.cpp

```cpp
// qt-ui/src/widgets/CaptureCard.cpp
#include "widgets/CaptureCard.h"

#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QPainter>
#include <QPixmap>
#include <QEnterEvent>

namespace sd {

CaptureCard::CaptureCard(const QString& title,
                         const QString& description,
                         const QString& iconPath,
                         QWidget* parent)
    : QWidget(parent)
    , m_title(title)
    , m_description(description)
    , m_iconPath(iconPath)
{
    setupUi();
    updateStyle(false);
    setCursor(Qt::PointingHandCursor);
    setMinimumSize(200, 220);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
}

CaptureCard::~CaptureCard() = default;

void CaptureCard::setupUi() {
    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(20, 24, 20, 20);
    layout->setSpacing(10);
    layout->setAlignment(Qt::AlignCenter);

    // Icon
    m_iconLabel = new QLabel(this);
    m_iconLabel->setAlignment(Qt::AlignCenter);
    QPixmap icon(m_iconPath);
    if (!icon.isNull()) {
        m_iconLabel->setPixmap(icon.scaled(48, 48, Qt::KeepAspectRatio,
                                            Qt::SmoothTransformation));
    } else {
        // Fallback: show title initial as placeholder
        m_iconLabel->setText(m_title.left(1));
        m_iconLabel->setStyleSheet(
            "font-size: 32px; font-weight: bold; color: #e94560;");
    }
    layout->addWidget(m_iconLabel);

    // Title
    m_titleLabel = new QLabel(m_title, this);
    m_titleLabel->setAlignment(Qt::AlignCenter);
    m_titleLabel->setStyleSheet(
        "font-size: 16px; font-weight: bold; color: #e0e0e0; background: transparent;");
    layout->addWidget(m_titleLabel);

    // Description
    m_descLabel = new QLabel(m_description, this);
    m_descLabel->setAlignment(Qt::AlignCenter);
    m_descLabel->setWordWrap(true);
    m_descLabel->setStyleSheet(
        "font-size: 12px; color: #8892a4; background: transparent;");
    layout->addWidget(m_descLabel);

    layout->addSpacing(8);

    // Buttons row
    auto* btnLayout = new QHBoxLayout();
    btnLayout->setSpacing(8);

    m_screenshotBtn = new QPushButton("Screenshot", this);
    m_screenshotBtn->setFixedHeight(34);
    m_screenshotBtn->setCursor(Qt::PointingHandCursor);
    m_screenshotBtn->setStyleSheet(
        "QPushButton {"
        "  background-color: #1f2b47; color: #e0e0e0;"
        "  border: 1px solid #2a3a5c; border-radius: 6px;"
        "  padding: 4px 12px; font-size: 12px;"
        "}"
        "QPushButton:hover { background-color: #2a3a5c; border-color: #e94560; }"
        "QPushButton:pressed { background-color: #e94560; color: #fff; }"
    );

    m_videoBtn = new QPushButton("Video", this);
    m_videoBtn->setFixedHeight(34);
    m_videoBtn->setCursor(Qt::PointingHandCursor);
    m_videoBtn->setObjectName("accentButton");
    m_videoBtn->setStyleSheet(
        "QPushButton {"
        "  background-color: #e94560; color: #ffffff;"
        "  border: none; border-radius: 6px;"
        "  padding: 4px 12px; font-size: 12px; font-weight: bold;"
        "}"
        "QPushButton:hover { background-color: #ff6b81; }"
        "QPushButton:pressed { background-color: #c73a52; }"
    );

    btnLayout->addWidget(m_screenshotBtn);
    btnLayout->addWidget(m_videoBtn);
    layout->addLayout(btnLayout);

    // Connect signals
    connect(m_screenshotBtn, &QPushButton::clicked,
            this, &CaptureCard::screenshotClicked);
    connect(m_videoBtn, &QPushButton::clicked,
            this, &CaptureCard::videoClicked);
}

void CaptureCard::updateStyle(bool hovered) {
    QString bg = hovered ? "#1f2b47" : "#16213e";
    QString border = hovered ? "#e94560" : "#2a3a5c";
    setStyleSheet(
        QString("sd--CaptureCard {"
                "  background-color: %1;"
                "  border: 1px solid %2;"
                "  border-radius: 12px;"
                "}").arg(bg, border));
}

void CaptureCard::enterEvent(QEnterEvent* event) {
    m_hovered = true;
    updateStyle(true);
    QWidget::enterEvent(event);
}

void CaptureCard::leaveEvent(QEvent* event) {
    m_hovered = false;
    updateStyle(false);
    QWidget::leaveEvent(event);
}

void CaptureCard::paintEvent(QPaintEvent* event) {
    // Required for stylesheet to work on custom QWidget subclass
    QStyleOption opt;
    opt.initFrom(this);
    QPainter p(this);
    style()->drawPrimitive(QStyle::PE_Widget, &opt, &p, this);
    QWidget::paintEvent(event);
}

} // namespace sd
```

---

### Task 16: RecentCaptures Widget

**Files:** `qt-ui/src/widgets/FlowLayout.h`, `qt-ui/src/widgets/FlowLayout.cpp`, `qt-ui/src/widgets/RecentCaptures.h`, `qt-ui/src/widgets/RecentCaptures.cpp`

#### Steps

- [ ] **16.1** Create `FlowLayout` (Qt's official flow layout example, adapted)
- [ ] **16.2** Create `RecentCaptures.h` with scroll area, context menu, and refresh logic
- [ ] **16.3** Implement `RecentCaptures.cpp` — scan output directory for images/videos, show thumbnails
- [ ] **16.4** Implement right-click context menu (Open, Copy Path, Delete, Add Note)
- [ ] **16.5** Double-click opens file in system viewer (or future EditorWindow)
- [ ] **16.6** Verify scrolling and thumbnail loading

#### FlowLayout.h

```cpp
// qt-ui/src/widgets/FlowLayout.h
// Adapted from Qt's official Flow Layout example (BSD license, compatible with GPLv3)
#pragma once

#include <QLayout>
#include <QStyle>

namespace sd {

class FlowLayout : public QLayout {
public:
    explicit FlowLayout(QWidget* parent = nullptr, int margin = -1,
                        int hSpacing = -1, int vSpacing = -1);
    ~FlowLayout() override;

    void addItem(QLayoutItem* item) override;
    int horizontalSpacing() const;
    int verticalSpacing() const;
    Qt::Orientations expandingDirections() const override;
    bool hasHeightForWidth() const override;
    int heightForWidth(int width) const override;
    int count() const override;
    QLayoutItem* itemAt(int index) const override;
    QSize minimumSize() const override;
    void setGeometry(const QRect& rect) override;
    QSize sizeHint() const override;
    QLayoutItem* takeAt(int index) override;

private:
    int doLayout(const QRect& rect, bool testOnly) const;
    int smartSpacing(QStyle::PixelMetric pm) const;

    QList<QLayoutItem*> m_itemList;
    int m_hSpace;
    int m_vSpace;
};

} // namespace sd
```

#### FlowLayout.cpp

```cpp
// qt-ui/src/widgets/FlowLayout.cpp
#include "widgets/FlowLayout.h"

#include <QWidget>

namespace sd {

FlowLayout::FlowLayout(QWidget* parent, int margin, int hSpacing, int vSpacing)
    : QLayout(parent), m_hSpace(hSpacing), m_vSpace(vSpacing)
{
    setContentsMargins(margin, margin, margin, margin);
}

FlowLayout::~FlowLayout() {
    QLayoutItem* item;
    while ((item = takeAt(0)))
        delete item;
}

void FlowLayout::addItem(QLayoutItem* item) {
    m_itemList.append(item);
}

int FlowLayout::horizontalSpacing() const {
    if (m_hSpace >= 0) return m_hSpace;
    return smartSpacing(QStyle::PM_LayoutHorizontalSpacing);
}

int FlowLayout::verticalSpacing() const {
    if (m_vSpace >= 0) return m_vSpace;
    return smartSpacing(QStyle::PM_LayoutVerticalSpacing);
}

int FlowLayout::count() const {
    return m_itemList.size();
}

QLayoutItem* FlowLayout::itemAt(int index) const {
    return m_itemList.value(index);
}

QLayoutItem* FlowLayout::takeAt(int index) {
    if (index >= 0 && index < m_itemList.size())
        return m_itemList.takeAt(index);
    return nullptr;
}

Qt::Orientations FlowLayout::expandingDirections() const {
    return {};
}

bool FlowLayout::hasHeightForWidth() const {
    return true;
}

int FlowLayout::heightForWidth(int width) const {
    return doLayout(QRect(0, 0, width, 0), true);
}

void FlowLayout::setGeometry(const QRect& rect) {
    QLayout::setGeometry(rect);
    doLayout(rect, false);
}

QSize FlowLayout::sizeHint() const {
    return minimumSize();
}

QSize FlowLayout::minimumSize() const {
    QSize size;
    for (const QLayoutItem* item : m_itemList)
        size = size.expandedTo(item->minimumSize());
    const auto margins = contentsMargins();
    size += QSize(margins.left() + margins.right(), margins.top() + margins.bottom());
    return size;
}

int FlowLayout::doLayout(const QRect& rect, bool testOnly) const {
    int left, top, right, bottom;
    getContentsMargins(&left, &top, &right, &bottom);
    QRect effectiveRect = rect.adjusted(+left, +top, -right, -bottom);
    int x = effectiveRect.x();
    int y = effectiveRect.y();
    int lineHeight = 0;

    for (QLayoutItem* item : m_itemList) {
        const QWidget* wid = item->widget();
        int spaceX = horizontalSpacing();
        if (spaceX == -1)
            spaceX = wid->style()->layoutSpacing(
                QSizePolicy::PushButton, QSizePolicy::PushButton, Qt::Horizontal);
        int spaceY = verticalSpacing();
        if (spaceY == -1)
            spaceY = wid->style()->layoutSpacing(
                QSizePolicy::PushButton, QSizePolicy::PushButton, Qt::Vertical);

        int nextX = x + item->sizeHint().width() + spaceX;
        if (nextX - spaceX > effectiveRect.right() && lineHeight > 0) {
            x = effectiveRect.x();
            y = y + lineHeight + spaceY;
            nextX = x + item->sizeHint().width() + spaceX;
            lineHeight = 0;
        }

        if (!testOnly)
            item->setGeometry(QRect(QPoint(x, y), item->sizeHint()));

        x = nextX;
        lineHeight = qMax(lineHeight, item->sizeHint().height());
    }
    return y + lineHeight - rect.y() + bottom;
}

int FlowLayout::smartSpacing(QStyle::PixelMetric pm) const {
    QObject* parent = this->parent();
    if (!parent) {
        return -1;
    } else if (parent->isWidgetType()) {
        auto* pw = static_cast<QWidget*>(parent);
        return pw->style()->pixelMetric(pm, nullptr, pw);
    } else {
        return static_cast<QLayout*>(parent)->spacing();
    }
}

} // namespace sd
```

#### RecentCaptures.h

```cpp
// qt-ui/src/widgets/RecentCaptures.h
#pragma once

#include <QWidget>
#include <QScrollArea>
#include <QLabel>
#include <QFileInfoList>
#include <QMenu>

namespace sd {

class FlowLayout;

class ThumbnailWidget : public QWidget {
    Q_OBJECT

public:
    explicit ThumbnailWidget(const QString& filePath, QWidget* parent = nullptr);
    QString filePath() const { return m_filePath; }

signals:
    void doubleClicked(const QString& filePath);
    void contextMenuRequested(const QString& filePath, const QPoint& globalPos);

protected:
    void mouseDoubleClickEvent(QMouseEvent* event) override;
    void contextMenuEvent(QContextMenuEvent* event) override;
    void enterEvent(QEnterEvent* event) override;
    void leaveEvent(QEvent* event) override;
    void paintEvent(QPaintEvent* event) override;

private:
    QString m_filePath;
    QLabel* m_thumbnail = nullptr;
    QLabel* m_nameLabel = nullptr;
    bool m_hovered = false;
};

class RecentCaptures : public QWidget {
    Q_OBJECT

public:
    explicit RecentCaptures(QWidget* parent = nullptr);
    ~RecentCaptures() override;

public slots:
    void refresh();

signals:
    void fileOpened(const QString& filePath);

private slots:
    void onContextMenu(const QString& filePath, const QPoint& globalPos);
    void onDoubleClicked(const QString& filePath);

private:
    void clearThumbnails();
    QFileInfoList scanOutputDirectory() const;

    QScrollArea* m_scrollArea = nullptr;
    QWidget* m_container = nullptr;
    FlowLayout* m_flowLayout = nullptr;
    QLabel* m_emptyLabel = nullptr;
};

} // namespace sd
```

#### RecentCaptures.cpp

```cpp
// qt-ui/src/widgets/RecentCaptures.cpp
#include "widgets/RecentCaptures.h"
#include "widgets/FlowLayout.h"
#include "core/AppState.h"

#include <QVBoxLayout>
#include <QScrollArea>
#include <QDir>
#include <QFileInfo>
#include <QPixmap>
#include <QImage>
#include <QDesktopServices>
#include <QUrl>
#include <QClipboard>
#include <QApplication>
#include <QMessageBox>
#include <QContextMenuEvent>
#include <QPainter>
#include <QStyleOption>
#include <QDateTime>
#include <QDebug>

namespace sd {

// ── ThumbnailWidget ─────────────────────────────────────────────────────────

static const int THUMB_WIDTH = 160;
static const int THUMB_HEIGHT = 120;

ThumbnailWidget::ThumbnailWidget(const QString& filePath, QWidget* parent)
    : QWidget(parent)
    , m_filePath(filePath)
{
    setFixedSize(THUMB_WIDTH, THUMB_HEIGHT + 28);
    setCursor(Qt::PointingHandCursor);

    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(4, 4, 4, 4);
    layout->setSpacing(4);

    // Thumbnail
    m_thumbnail = new QLabel(this);
    m_thumbnail->setFixedSize(THUMB_WIDTH - 8, THUMB_HEIGHT - 8);
    m_thumbnail->setAlignment(Qt::AlignCenter);
    m_thumbnail->setStyleSheet("background-color: #1a1a2e; border-radius: 4px;");

    QFileInfo fi(filePath);
    QString suffix = fi.suffix().toLower();

    if (suffix == "png" || suffix == "jpg" || suffix == "jpeg" || suffix == "bmp") {
        QPixmap pix(filePath);
        if (!pix.isNull()) {
            m_thumbnail->setPixmap(pix.scaled(
                THUMB_WIDTH - 8, THUMB_HEIGHT - 8,
                Qt::KeepAspectRatio, Qt::SmoothTransformation));
        } else {
            m_thumbnail->setText("?");
        }
    } else {
        // Video or other — show file extension as placeholder
        m_thumbnail->setText(suffix.toUpper());
        m_thumbnail->setStyleSheet(
            "background-color: #1a1a2e; border-radius: 4px;"
            "font-size: 18px; font-weight: bold; color: #e94560;");
    }

    layout->addWidget(m_thumbnail);

    // Filename label
    m_nameLabel = new QLabel(fi.fileName(), this);
    m_nameLabel->setAlignment(Qt::AlignCenter);
    m_nameLabel->setStyleSheet(
        "font-size: 10px; color: #8892a4; background: transparent;");
    m_nameLabel->setMaximumWidth(THUMB_WIDTH - 8);

    QFontMetrics fm(m_nameLabel->font());
    m_nameLabel->setText(fm.elidedText(fi.fileName(), Qt::ElideMiddle, THUMB_WIDTH - 16));

    layout->addWidget(m_nameLabel);

    setStyleSheet("sd--ThumbnailWidget {"
                  "  background-color: #16213e;"
                  "  border: 1px solid #2a3a5c;"
                  "  border-radius: 8px;"
                  "}");
}

void ThumbnailWidget::mouseDoubleClickEvent(QMouseEvent* event) {
    emit doubleClicked(m_filePath);
    QWidget::mouseDoubleClickEvent(event);
}

void ThumbnailWidget::contextMenuEvent(QContextMenuEvent* event) {
    emit contextMenuRequested(m_filePath, event->globalPos());
    event->accept();
}

void ThumbnailWidget::enterEvent(QEnterEvent* event) {
    m_hovered = true;
    setStyleSheet("sd--ThumbnailWidget {"
                  "  background-color: #1f2b47;"
                  "  border: 1px solid #e94560;"
                  "  border-radius: 8px;"
                  "}");
    QWidget::enterEvent(event);
}

void ThumbnailWidget::leaveEvent(QEvent* event) {
    m_hovered = false;
    setStyleSheet("sd--ThumbnailWidget {"
                  "  background-color: #16213e;"
                  "  border: 1px solid #2a3a5c;"
                  "  border-radius: 8px;"
                  "}");
    QWidget::leaveEvent(event);
}

void ThumbnailWidget::paintEvent(QPaintEvent* event) {
    QStyleOption opt;
    opt.initFrom(this);
    QPainter p(this);
    style()->drawPrimitive(QStyle::PE_Widget, &opt, &p, this);
    QWidget::paintEvent(event);
}

// ── RecentCaptures ──────────────────────────────────────────────────────────

RecentCaptures::RecentCaptures(QWidget* parent)
    : QWidget(parent)
{
    auto* mainLayout = new QVBoxLayout(this);
    mainLayout->setContentsMargins(0, 0, 0, 0);

    m_scrollArea = new QScrollArea(this);
    m_scrollArea->setWidgetResizable(true);
    m_scrollArea->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_scrollArea->setVerticalScrollBarPolicy(Qt::ScrollBarAsNeeded);
    m_scrollArea->setStyleSheet("QScrollArea { border: none; background: transparent; }");

    m_container = new QWidget(m_scrollArea);
    m_flowLayout = new FlowLayout(m_container, 8, 8, 8);
    m_container->setLayout(m_flowLayout);

    m_scrollArea->setWidget(m_container);
    mainLayout->addWidget(m_scrollArea);

    // Empty state label
    m_emptyLabel = new QLabel("No recent captures yet.\nTake a screenshot or record a video to get started!", this);
    m_emptyLabel->setAlignment(Qt::AlignCenter);
    m_emptyLabel->setStyleSheet("color: #555b6e; font-size: 14px; background: transparent;");
    mainLayout->addWidget(m_emptyLabel);

    refresh();
}

RecentCaptures::~RecentCaptures() = default;

QFileInfoList RecentCaptures::scanOutputDirectory() const {
    QString dir = AppState::instance()->outputDirectory();
    QDir outputDir(dir);

    QStringList filters;
    filters << "*.png" << "*.jpg" << "*.jpeg" << "*.bmp"
            << "*.mp4" << "*.webm" << "*.mkv" << "*.avi";

    QFileInfoList files = outputDir.entryInfoList(filters,
        QDir::Files | QDir::Readable, QDir::Time);

    // Limit to most recent 50
    if (files.size() > 50)
        files = files.mid(0, 50);

    return files;
}

void RecentCaptures::clearThumbnails() {
    QLayoutItem* item;
    while ((item = m_flowLayout->takeAt(0))) {
        if (item->widget())
            item->widget()->deleteLater();
        delete item;
    }
}

void RecentCaptures::refresh() {
    clearThumbnails();

    QFileInfoList files = scanOutputDirectory();

    bool empty = files.isEmpty();
    m_emptyLabel->setVisible(empty);
    m_scrollArea->setVisible(!empty);

    for (const QFileInfo& fi : files) {
        auto* thumb = new ThumbnailWidget(fi.absoluteFilePath(), m_container);

        connect(thumb, &ThumbnailWidget::doubleClicked,
                this, &RecentCaptures::onDoubleClicked);
        connect(thumb, &ThumbnailWidget::contextMenuRequested,
                this, &RecentCaptures::onContextMenu);

        m_flowLayout->addWidget(thumb);
    }
}

void RecentCaptures::onDoubleClicked(const QString& filePath) {
    QDesktopServices::openUrl(QUrl::fromLocalFile(filePath));
    emit fileOpened(filePath);
}

void RecentCaptures::onContextMenu(const QString& filePath, const QPoint& globalPos) {
    QMenu menu;

    QAction* openAction = menu.addAction("Open");
    QAction* copyPathAction = menu.addAction("Copy Path");
    menu.addSeparator();
    QAction* deleteAction = menu.addAction("Delete");

    QAction* chosen = menu.exec(globalPos);

    if (chosen == openAction) {
        QDesktopServices::openUrl(QUrl::fromLocalFile(filePath));
    } else if (chosen == copyPathAction) {
        QApplication::clipboard()->setText(filePath);
    } else if (chosen == deleteAction) {
        auto reply = QMessageBox::question(this, "Delete Capture",
            QString("Delete %1?").arg(QFileInfo(filePath).fileName()),
            QMessageBox::Yes | QMessageBox::No);
        if (reply == QMessageBox::Yes) {
            QFile::remove(filePath);
            refresh();
        }
    }
}

} // namespace sd
```

---

## Build & Verify Checklist

After implementing all tasks:

- [ ] **B.1** Ensure `qt-ui/include/screen_dream_ffi.h` exists (from cbindgen in Plan 5a)
- [ ] **B.2** Run `cd qt-ui && cmake -B build -DCMAKE_BUILD_TYPE=Release`
- [ ] **B.3** Run `cmake --build build --parallel $(nproc)`
- [ ] **B.4** Run `./build/ScreenDream` — verify window appears with dark theme
- [ ] **B.5** Verify three capture cards render with hover effects
- [ ] **B.6** Verify status bar shows FFmpeg and platform info
- [ ] **B.7** Verify system tray icon appears
- [ ] **B.8** Verify Quick Screenshot (Ctrl+Shift+S) writes a file to the output directory
- [ ] **B.9** Verify RecentCaptures populates after a screenshot is taken

---

## Dependencies

| Task | Depends On |
|------|-----------|
| 9 (CMakeLists.txt) | Plan 5a (FFI crate exists, cbindgen header generated) |
| 10 (RustBridge) | Task 9, `screen_dream_ffi.h` |
| 11 (AppState) | Task 10 |
| 12 (main.cpp) | Tasks 10, 11, 13, 14 |
| 13 (Dark Theme) | None (standalone resource) |
| 14 (MainWindow) | Tasks 11, 15, 16 |
| 15 (CaptureCard) | Task 13 (theme) |
| 16 (RecentCaptures) | Task 11 (AppState for output dir) |

**Recommended implementation order:** 13 -> 9 -> 10 -> 11 -> 15 -> 16 -> 14 -> 12
