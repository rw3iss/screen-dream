#ifndef MAINWINDOW_H
#define MAINWINDOW_H

#include <QMainWindow>
#include <QLabel>
#include <QPushButton>
#include <QMenuBar>
#include <QStatusBar>

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
};

#endif // MAINWINDOW_H
