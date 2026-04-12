#include "core/RustBridge.h"

#include <QJsonDocument>
#include <QJsonArray>
#include <QJsonObject>
#include <QByteArray>
#include <cstring>

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

static QString fromCStr(const char *s) {
    return s ? QString::fromUtf8(s) : QString();
}

/// Take ownership of a Rust-allocated string, convert to QString, then free.
static QString takeCStr(char *s) {
    if (!s) return QString();
    QString result = QString::fromUtf8(s);
    sd_free_string(s);
    return result;
}

void RustBridge::checkError(SDError *err) {
    if (!err) return;
    QString msg = fromCStr(err->message);
    sd_free_error(err);
    throw std::runtime_error(msg.toStdString());
}

SDCaptureSource RustBridge::toSDCaptureSource(const CaptureSource &src) {
    SDCaptureSource cs{};
    cs.source_type   = static_cast<uint32_t>(src.type);
    cs.monitor_id    = src.monitorId;
    cs.window_id     = src.windowId;
    cs.region_x      = src.regionX;
    cs.region_y      = src.regionY;
    cs.region_width  = src.regionWidth;
    cs.region_height = src.regionHeight;
    return cs;
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

RustBridge::RustBridge(const QString &configDir) {
    QByteArray utf8 = configDir.toUtf8();
    SDError *err = nullptr;
    if (!sd_init(utf8.constData(), &err)) {
        checkError(err);
        // If checkError didn't throw (shouldn't happen), throw generic.
        throw std::runtime_error("sd_init failed with unknown error");
    }
}

RustBridge::~RustBridge() {
    sd_shutdown();
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

PlatformInfo RustBridge::getPlatformInfo() {
    SDPlatformInfo raw = sd_get_platform_info();
    PlatformInfo info;
    info.os            = takeCStr(raw.os);
    info.displayServer = takeCStr(raw.display_server);
    info.arch          = takeCStr(raw.arch);
    return info;
}

AvailableSources RustBridge::enumerateSources() {
    SDError *err = nullptr;
    SDAvailableSources *raw = sd_enumerate_sources(&err);
    checkError(err);
    if (!raw) throw std::runtime_error("sd_enumerate_sources returned null without error");

    AvailableSources result;
    result.windowsUnavailable       = raw->windows_unavailable;
    result.windowsUnavailableReason = fromCStr(raw->windows_unavailable_reason);

    for (uint32_t i = 0; i < raw->monitors_count; ++i) {
        const SDMonitorInfo &m = raw->monitors[i];
        MonitorInfo mi;
        mi.id           = m.id;
        mi.name         = fromCStr(m.name);
        mi.friendlyName = fromCStr(m.friendly_name);
        mi.width        = m.width;
        mi.height       = m.height;
        mi.x            = m.x;
        mi.y            = m.y;
        mi.scaleFactor  = m.scale_factor;
        mi.isPrimary    = m.is_primary;
        result.monitors.append(mi);
    }

    for (uint32_t i = 0; i < raw->windows_count; ++i) {
        const SDWindowInfo &w = raw->windows[i];
        WindowInfo wi;
        wi.id          = w.id;
        wi.pid         = w.pid;
        wi.appName     = fromCStr(w.app_name);
        wi.title       = fromCStr(w.title);
        wi.width       = w.width;
        wi.height      = w.height;
        wi.isMinimized = w.is_minimized;
        wi.isFocused   = w.is_focused;
        result.windows.append(wi);
    }

    sd_free_available_sources(raw);
    return result;
}

FfmpegStatus RustBridge::getFfmpegStatus() {
    SDError *err = nullptr;
    SDFfmpegStatus *raw = sd_get_ffmpeg_status(&err);
    checkError(err);
    if (!raw) throw std::runtime_error("sd_get_ffmpeg_status returned null without error");

    FfmpegStatus status;
    status.available         = raw->available;
    status.version           = fromCStr(raw->version);
    status.sourceDescription = fromCStr(raw->source_description);

    // Parse JSON arrays of encoder names
    auto parseEncoders = [](const char *json) -> QStringList {
        QStringList list;
        if (!json) return list;
        QJsonDocument doc = QJsonDocument::fromJson(QByteArray(json));
        if (doc.isArray()) {
            for (const QJsonValue &v : doc.array()) {
                if (v.isString()) list.append(v.toString());
            }
        }
        return list;
    };
    status.videoEncoders = parseEncoders(raw->video_encoders_json);
    status.audioEncoders = parseEncoders(raw->audio_encoders_json);

    sd_free_ffmpeg_status(raw);
    return status;
}

QJsonObject RustBridge::loadSettings() {
    SDError *err = nullptr;
    char *json = sd_load_settings(&err);
    checkError(err);
    QString str = takeCStr(json);
    QJsonDocument doc = QJsonDocument::fromJson(str.toUtf8());
    return doc.object();
}

void RustBridge::saveSettings(const QJsonObject &settings) {
    QJsonDocument doc(settings);
    QByteArray json = doc.toJson(QJsonDocument::Compact);
    SDError *err = nullptr;
    if (!sd_save_settings(json.constData(), &err)) {
        checkError(err);
        throw std::runtime_error("sd_save_settings failed with unknown error");
    }
}

QJsonObject RustBridge::resetSettings() {
    SDError *err = nullptr;
    char *json = sd_reset_settings(&err);
    checkError(err);
    QString str = takeCStr(json);
    QJsonDocument doc = QJsonDocument::fromJson(str.toUtf8());
    return doc.object();
}

QVector<AudioDeviceInfo> RustBridge::listAudioDevices() {
    SDError *err = nullptr;
    char *json = sd_list_audio_devices(&err);
    checkError(err);
    QString str = takeCStr(json);

    QVector<AudioDeviceInfo> devices;
    QJsonDocument doc = QJsonDocument::fromJson(str.toUtf8());
    if (!doc.isArray()) return devices;

    for (const QJsonValue &v : doc.array()) {
        QJsonObject obj = v.toObject();
        AudioDeviceInfo info;
        info.name       = obj.value("name").toString();
        info.isDefault  = obj.value("is_default").toBool();
        info.sampleRate = static_cast<uint32_t>(obj.value("sample_rate").toInt());
        info.channels   = static_cast<uint16_t>(obj.value("channels").toInt());
        devices.append(info);
    }
    return devices;
}

// ---------------------------------------------------------------------------
// Capture
// ---------------------------------------------------------------------------

QString RustBridge::takeScreenshot(const CaptureSource &source, const QString &path) {
    SDCaptureSource cs = toSDCaptureSource(source);
    QByteArray pathUtf8 = path.toUtf8();
    SDError *err = nullptr;
    if (!sd_take_screenshot(&cs, pathUtf8.constData(), &err)) {
        checkError(err);
        throw std::runtime_error("sd_take_screenshot failed with unknown error");
    }
    return path;
}

QByteArray RustBridge::takeScreenshotBase64(const CaptureSource &source) {
    SDCaptureSource cs = toSDCaptureSource(source);
    SDError *err = nullptr;
    char *b64 = sd_take_screenshot_base64(&cs, &err);
    checkError(err);
    if (!b64) throw std::runtime_error("sd_take_screenshot_base64 returned null without error");
    QByteArray result(b64);
    sd_free_string(b64);
    return result;
}

QImage RustBridge::captureFrame(const CaptureSource &source) {
    SDCaptureSource cs = toSDCaptureSource(source);
    SDError *err = nullptr;
    SDFrame *frame = sd_capture_frame(&cs, &err);
    checkError(err);
    if (!frame) throw std::runtime_error("sd_capture_frame returned null without error");

    // SDFrame contains RGBA data. Create a deep copy so we can free the frame.
    QImage img(frame->data, frame->width, frame->height, QImage::Format_RGBA8888);
    QImage copy = img.copy(); // deep copy before freeing
    sd_free_frame(frame);
    return copy;
}

// ---------------------------------------------------------------------------
// Recording
// ---------------------------------------------------------------------------

SDRecordingHandle *RustBridge::startRecording(const RecordingConfig &config) {
    SDCaptureSource cs = toSDCaptureSource(config.source);

    QByteArray codecUtf8  = config.videoCodec.toUtf8();
    QByteArray presetUtf8 = config.preset.toUtf8();
    QByteArray pathUtf8   = config.outputPath.toUtf8();
    QByteArray micUtf8    = config.microphoneDevice.toUtf8();

    SDRecordingConfig rc{};
    rc.source              = cs;
    rc.fps                 = config.fps;
    rc.video_codec         = codecUtf8.constData();
    rc.crf                 = config.crf;
    rc.preset              = presetUtf8.constData();
    rc.output_path         = pathUtf8.constData();
    rc.capture_microphone  = config.captureMicrophone;
    rc.microphone_device   = micUtf8.constData();

    SDError *err = nullptr;
    SDRecordingHandle *handle = sd_start_recording(&rc, &err);
    checkError(err);
    if (!handle) throw std::runtime_error("sd_start_recording returned null without error");
    return handle;
}

QString RustBridge::stopRecording(SDRecordingHandle *handle) {
    char *outPath = nullptr;
    SDError *err = nullptr;
    if (!sd_stop_recording(handle, &outPath, &err)) {
        checkError(err);
        throw std::runtime_error("sd_stop_recording failed with unknown error");
    }
    return takeCStr(outPath);
}

void RustBridge::pauseRecording(SDRecordingHandle *handle) {
    SDError *err = nullptr;
    if (!sd_pause_recording(handle, &err)) {
        checkError(err);
        throw std::runtime_error("sd_pause_recording failed with unknown error");
    }
}

void RustBridge::resumeRecording(SDRecordingHandle *handle) {
    SDError *err = nullptr;
    if (!sd_resume_recording(handle, &err)) {
        checkError(err);
        throw std::runtime_error("sd_resume_recording failed with unknown error");
    }
}

RecordingStatus RustBridge::getRecordingStatus(SDRecordingHandle *handle) {
    SDRecordingStatus raw = sd_get_recording_status(handle);
    RecordingStatus status;
    status.state          = static_cast<RecordingStatus::State>(raw.state);
    status.elapsedSeconds = raw.elapsed_seconds;
    status.framesCaptured = raw.frames_captured;
    return status;
}

void RustBridge::freeRecordingHandle(SDRecordingHandle *handle) {
    sd_free_recording_handle(handle);
}
