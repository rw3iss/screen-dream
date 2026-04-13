#ifndef MAINWINDOW_H
#define MAINWINDOW_H

#include <QMainWindow>
#include <QLabel>
#include <QPushButton>
#include <QMenuBar>
#include <QStatusBar>
#include <QString>
#include <QTimer>
#include <QElapsedTimer>

class CaptureCard;
class RecentCaptures;
class SourceBrowser;
class RegionPicker;

struct CaptureSource;
struct RecordingStatus;

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(QWidget *parent = nullptr);

private slots:
    void onSettingsClicked();
    void onAbout();
    void onSourceSelected(const CaptureSource &source);
    void onScreenScreenshot();
    void onWindowScreenshot();
    void onAreaScreenshot();
    void onScreenRecord();
    void onWindowRecord();
    void onAreaRecord();
    void onRecordingStateChanged(const RecordingStatus &status);
    void onRecordingTimerTick();
    void onStatusMessageChanged(const QString &message);
    void onCopyStatusMessage();

private:
    void setupMenuBar();
    void setupCentralWidget();
    void setupStatusBar();

    /// Start recording with the given source. Shared by all three Record buttons.
    void startRecording(const CaptureSource &source);
    /// Stop the current recording.
    void stopRecording();
    /// Update all card buttons to reflect recording state.
    void setAllCardsRecordingState(bool recording);

    // Capture cards
    CaptureCard *m_screenCard;
    CaptureCard *m_windowCard;
    CaptureCard *m_areaCard;

    // Source browser
    SourceBrowser *m_sourceBrowser;

    // Recent captures
    RecentCaptures *m_recentCaptures;

    // Currently selected capture source
    CaptureSource *m_selectedSource;

    // Status bar widgets
    QPushButton *m_settingsBtn;
    QLabel *m_ffmpegLabel;
    QLabel *m_platformLabel;
    QPushButton *m_copyStatusBtn;
    QString m_lastStatusMessage;

    // Recording state
    bool m_isRecording = false;
    QTimer *m_recordingTimer = nullptr;
    QElapsedTimer m_recordingElapsed;
    QLabel *m_recordingTimeLabel = nullptr;
};

#endif // MAINWINDOW_H
