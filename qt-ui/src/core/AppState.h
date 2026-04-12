#ifndef APPSTATE_H
#define APPSTATE_H

#include <QObject>
#include <memory>
#include "core/RustBridge.h"

class AppState : public QObject {
    Q_OBJECT

public:
    /// Get the singleton instance. Must call init() first.
    static AppState &instance();

    /// Initialize with the given config directory. Call once at startup.
    static void init(const QString &configDir);

    /// Access the underlying RustBridge.
    RustBridge &bridge() { return *m_bridge; }

    /// Current recording handle (nullptr when idle).
    SDRecordingHandle *recordingHandle() const { return m_recordingHandle; }

signals:
    void recordingStateChanged(RecordingStatus status);
    void settingsChanged();

public slots:
    void startRecording(const RecordingConfig &config);
    QString stopRecording();
    void pauseRecording();
    void resumeRecording();

    /// Poll current recording status and emit recordingStateChanged.
    void refreshRecordingStatus();

private:
    explicit AppState(const QString &configDir, QObject *parent = nullptr);
    ~AppState() override;

    AppState(const AppState &) = delete;
    AppState &operator=(const AppState &) = delete;

    std::unique_ptr<RustBridge> m_bridge;
    SDRecordingHandle *m_recordingHandle = nullptr;

    static AppState *s_instance;
};

#endif // APPSTATE_H
