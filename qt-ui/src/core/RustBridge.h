#ifndef RUSTBRIDGE_H
#define RUSTBRIDGE_H

#include <QString>
#include <QVector>
#include <QJsonObject>
#include <QJsonArray>
#include <QJsonDocument>
#include <QImage>
#include <QByteArray>
#include <QStringList>
#include <stdexcept>
#include <cstdint>

// Forward-declare the C FFI types so we don't leak them into every TU.
extern "C" {
#include "screen_dream_ffi.h"
}

// ---- C++ mirror structs -------------------------------------------------------

struct PlatformInfo {
    QString os;
    QString displayServer;
    QString arch;
};

struct MonitorInfo {
    uint32_t id;
    QString name;
    QString friendlyName;
    uint32_t width;
    uint32_t height;
    int32_t x;
    int32_t y;
    float scaleFactor;
    bool isPrimary;
};

struct WindowInfo {
    uint32_t id;
    uint32_t pid;
    QString appName;
    QString title;
    uint32_t width;
    uint32_t height;
    bool isMinimized;
    bool isFocused;
};

struct AvailableSources {
    QVector<MonitorInfo> monitors;
    QVector<WindowInfo> windows;
    bool windowsUnavailable;
    QString windowsUnavailableReason;
};

struct CaptureSource {
    enum Type { Screen = 0, Window = 1, Region = 2 };
    Type type = Screen;
    uint32_t monitorId = 0;
    uint32_t windowId = 0;
    int32_t regionX = 0;
    int32_t regionY = 0;
    uint32_t regionWidth = 0;
    uint32_t regionHeight = 0;
};

struct RecordingConfig {
    CaptureSource source;
    uint32_t fps = 30;
    QString videoCodec = QStringLiteral("libx264");
    uint8_t crf = 23;
    QString preset = QStringLiteral("ultrafast");
    QString outputPath;
    bool captureMicrophone = false;
    QString microphoneDevice;
};

struct RecordingStatus {
    enum State {
        Idle = 0,
        Starting = 1,
        Recording = 2,
        Paused = 3,
        Stopping = 4,
        Completed = 5,
        Failed = 6
    };
    State state = Idle;
    double elapsedSeconds = 0.0;
    uint64_t framesCaptured = 0;
};

struct FfmpegStatus {
    bool available = false;
    QString version;
    QString sourceDescription;
    QStringList videoEncoders;
    QStringList audioEncoders;
};

struct AudioDeviceInfo {
    QString name;
    bool isDefault = false;
    uint32_t sampleRate = 0;
    uint16_t channels = 0;
};

// ---- RustBridge ---------------------------------------------------------------

class RustBridge {
public:
    /// Initializes the Rust core. Throws std::runtime_error on failure.
    explicit RustBridge(const QString &configDir);

    /// Shuts down the Rust core.
    ~RustBridge();

    // Non-copyable, non-movable (singleton-ish usage via AppState).
    RustBridge(const RustBridge &) = delete;
    RustBridge &operator=(const RustBridge &) = delete;

    // ---- queries ----
    PlatformInfo getPlatformInfo();
    AvailableSources enumerateSources();
    FfmpegStatus getFfmpegStatus();
    QJsonObject loadSettings();
    void saveSettings(const QJsonObject &settings);
    QJsonObject resetSettings();
    QVector<AudioDeviceInfo> listAudioDevices();

    // ---- capture ----
    QString takeScreenshot(const CaptureSource &source, const QString &path);
    QByteArray takeScreenshotBase64(const CaptureSource &source);
    QImage captureFrame(const CaptureSource &source);

    // ---- recording ----
    SDRecordingHandle *startRecording(const RecordingConfig &config);
    QString stopRecording(SDRecordingHandle *handle);
    void pauseRecording(SDRecordingHandle *handle);
    void resumeRecording(SDRecordingHandle *handle);
    RecordingStatus getRecordingStatus(SDRecordingHandle *handle);
    void freeRecordingHandle(SDRecordingHandle *handle);

private:
    /// Convert C++ CaptureSource to FFI SDCaptureSource.
    static SDCaptureSource toSDCaptureSource(const CaptureSource &src);

    /// Check an SDError pointer; if non-null, extract message, free error, throw.
    static void checkError(SDError *err);
};

#endif // RUSTBRIDGE_H
