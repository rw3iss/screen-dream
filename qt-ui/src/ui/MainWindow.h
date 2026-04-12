#ifndef MAINWINDOW_H
#define MAINWINDOW_H

#include <QMainWindow>
#include <QLabel>
#include <QPushButton>
#include <QMenuBar>
#include <QStatusBar>
#include <QString>

class CaptureCard;
class RecentCaptures;
class SourceBrowser;

struct CaptureSource;

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
    void onStatusMessageChanged(const QString &message);
    void onCopyStatusMessage();

private:
    void setupMenuBar();
    void setupCentralWidget();
    void setupStatusBar();

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
};

#endif // MAINWINDOW_H
