#include "core/AppState.h"
#include <QDebug>
#include <stdexcept>

AppState *AppState::s_instance = nullptr;

AppState &AppState::instance() {
    if (!s_instance) {
        throw std::runtime_error("AppState::init() must be called before instance()");
    }
    return *s_instance;
}

void AppState::init(const QString &configDir) {
    if (s_instance) {
        qWarning() << "AppState::init() called more than once; ignoring.";
        return;
    }
    s_instance = new AppState(configDir);
}

AppState::AppState(const QString &configDir, QObject *parent)
    : QObject(parent)
    , m_bridge(std::make_unique<RustBridge>(configDir))
{
}

AppState::~AppState() {
    if (m_recordingHandle) {
        try {
            m_bridge->stopRecording(m_recordingHandle);
        } catch (...) {
            // Best-effort cleanup
        }
        m_bridge->freeRecordingHandle(m_recordingHandle);
        m_recordingHandle = nullptr;
    }
}

void AppState::startRecording(const RecordingConfig &config) {
    if (m_recordingHandle) {
        qWarning() << "Recording already in progress";
        return;
    }
    m_recordingHandle = m_bridge->startRecording(config);
    refreshRecordingStatus();
}

QString AppState::stopRecording() {
    if (!m_recordingHandle) {
        qWarning() << "No recording in progress";
        return {};
    }
    QString path = m_bridge->stopRecording(m_recordingHandle);
    m_bridge->freeRecordingHandle(m_recordingHandle);
    m_recordingHandle = nullptr;

    RecordingStatus status;
    status.state = RecordingStatus::Completed;
    emit recordingStateChanged(status);

    return path;
}

void AppState::pauseRecording() {
    if (!m_recordingHandle) {
        qWarning() << "No recording in progress";
        return;
    }
    m_bridge->pauseRecording(m_recordingHandle);
    refreshRecordingStatus();
}

void AppState::resumeRecording() {
    if (!m_recordingHandle) {
        qWarning() << "No recording in progress";
        return;
    }
    m_bridge->resumeRecording(m_recordingHandle);
    refreshRecordingStatus();
}

void AppState::refreshRecordingStatus() {
    if (!m_recordingHandle) return;
    RecordingStatus status = m_bridge->getRecordingStatus(m_recordingHandle);
    emit recordingStateChanged(status);
}
