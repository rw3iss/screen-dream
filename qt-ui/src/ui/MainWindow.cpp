#include "ui/MainWindow.h"
#include "widgets/CaptureCard.h"
#include "widgets/SourceBrowser.h"
#include "widgets/RecentCaptures.h"
#include "core/AppState.h"

#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QWidget>
#include <QLabel>
#include <QMenuBar>
#include <QMenu>
#include <QAction>
#include <QStatusBar>
#include <QMessageBox>
#include <QDateTime>
#include <QDesktopServices>
#include <QDir>
#include <QUrl>
#include <QApplication>
#include <QClipboard>
#include <QJsonObject>
#include <QFrame>
#include <QTimer>
#include <QElapsedTimer>

MainWindow::MainWindow(QWidget *parent)
    : QMainWindow(parent), m_selectedSource(nullptr)
{
    setWindowTitle("Screen Dream");
    resize(800, 600);
    setMinimumSize(640, 480);

    setupMenuBar();
    setupCentralWidget();
    setupStatusBar();

    // Recording elapsed-time timer (fires every second while recording)
    m_recordingTimer = new QTimer(this);
    m_recordingTimer->setInterval(1000);
    connect(m_recordingTimer, &QTimer::timeout, this, &MainWindow::onRecordingTimerTick);

    // Listen for recording state changes from AppState
    connect(&AppState::instance(), &AppState::recordingStateChanged,
            this, &MainWindow::onRecordingStateChanged);
}

// ---------------------------------------------------------------------------
// Menu bar
// ---------------------------------------------------------------------------

void MainWindow::setupMenuBar()
{
    QMenuBar *mb = menuBar();

    // File menu
    QMenu *fileMenu = mb->addMenu("&File");
    QAction *quitAct = fileMenu->addAction("&Quit");
    quitAct->setShortcut(QKeySequence::Quit);
    connect(quitAct, &QAction::triggered, qApp, &QApplication::quit);

    // View menu
    QMenu *viewMenu = mb->addMenu("&View");
    QAction *settingsAct = viewMenu->addAction("&Settings");
    connect(settingsAct, &QAction::triggered, this, &MainWindow::onSettingsClicked);

    // Help menu
    QMenu *helpMenu = mb->addMenu("&Help");
    QAction *aboutAct = helpMenu->addAction("&About");
    connect(aboutAct, &QAction::triggered, this, &MainWindow::onAbout);
}

// ---------------------------------------------------------------------------
// Central widget
// ---------------------------------------------------------------------------

void MainWindow::setupCentralWidget()
{
    auto *central = new QWidget(this);
    auto *mainLayout = new QVBoxLayout(central);
    mainLayout->setContentsMargins(12, 6, 12, 6);
    mainLayout->setSpacing(4);

    // --- Capture cards row (fixed height, stretch to fill width equally) ---
    auto *cardsLayout = new QHBoxLayout();
    cardsLayout->setSpacing(8);

    m_screenCard = new CaptureCard(CaptureCard::Screen, central);
    m_windowCard = new CaptureCard(CaptureCard::Window, central);
    m_areaCard   = new CaptureCard(CaptureCard::Area, central);

    cardsLayout->addWidget(m_screenCard, 1);
    cardsLayout->addWidget(m_windowCard, 1);
    cardsLayout->addWidget(m_areaCard, 1);

    // Connect card buttons
    connect(m_screenCard, &CaptureCard::screenshotClicked, this, &MainWindow::onScreenScreenshot);
    connect(m_screenCard, &CaptureCard::recordClicked, this, &MainWindow::onScreenRecord);
    connect(m_windowCard, &CaptureCard::screenshotClicked, this, &MainWindow::onWindowScreenshot);
    connect(m_windowCard, &CaptureCard::recordClicked, this, &MainWindow::onWindowRecord);
    connect(m_areaCard, &CaptureCard::screenshotClicked, this, &MainWindow::onAreaScreenshot);
    connect(m_areaCard, &CaptureCard::recordClicked, this, &MainWindow::onAreaRecord);

    mainLayout->addLayout(cardsLayout, 0);  // no stretch — fixed height

    // --- Source browser (collapsible, takes all remaining space) ---
    m_sourceBrowser = new SourceBrowser(central);
    m_sourceBrowser->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
    mainLayout->addWidget(m_sourceBrowser, 1);  // stretch=1 — fills remaining height
    connect(m_sourceBrowser, &SourceBrowser::sourceSelected,
            this, &MainWindow::onSourceSelected);

    // --- Recent captures (collapsible footer) ---
    m_recentCaptures = new RecentCaptures(central);
    m_recentCaptures->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    mainLayout->addWidget(m_recentCaptures, 0);  // no stretch — stays at bottom

    // Load output directory from settings
    try {
        QJsonObject settings = AppState::instance().bridge().loadSettings();
        QString outputDir = settings.value("output_directory").toString();
        if (outputDir.isEmpty()) outputDir = QDir::homePath() + "/Pictures";
        m_recentCaptures->setDirectory(outputDir);
    } catch (const std::exception &e) {
        m_recentCaptures->setDirectory("/tmp");
    }

    central->setLayout(mainLayout);
    setCentralWidget(central);
}

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

void MainWindow::setupStatusBar()
{
    QStatusBar *sb = statusBar();

    // Settings button
    m_settingsBtn = new QPushButton("Settings", this);
    m_settingsBtn->setFlat(true);
    m_settingsBtn->setCursor(Qt::PointingHandCursor);
    m_settingsBtn->setStyleSheet(
        "QPushButton { color: #a0a0a0; border: none; padding: 2px 8px; font-size: 13px; background: transparent; }"
        "QPushButton:hover { color: #e0e0e0; }"
    );
    connect(m_settingsBtn, &QPushButton::clicked, this, &MainWindow::onSettingsClicked);
    sb->addWidget(m_settingsBtn);

    // FFmpeg status
    m_ffmpegLabel = new QLabel(this);
    m_ffmpegLabel->setStyleSheet("font-size: 12px;");
    try {
        FfmpegStatus ff = AppState::instance().bridge().getFfmpegStatus();
        if (ff.available) {
            m_ffmpegLabel->setText(QString("FFmpeg: %1").arg(ff.version));
            m_ffmpegLabel->setStyleSheet("color: #00d26a; font-size: 12px;");
        } else {
            m_ffmpegLabel->setText("FFmpeg: not found");
            m_ffmpegLabel->setStyleSheet("color: #e94560; font-size: 12px;");
        }
    } catch (...) {
        m_ffmpegLabel->setText("FFmpeg: unknown");
        m_ffmpegLabel->setStyleSheet("color: #606060; font-size: 12px;");
    }
    sb->addPermanentWidget(m_ffmpegLabel);

    // Copy-to-clipboard button (hidden by default)
    m_copyStatusBtn = new QPushButton(QString::fromUtf8("\xF0\x9F\x93\x8B"), this);
    m_copyStatusBtn->setFlat(true);
    m_copyStatusBtn->setCursor(Qt::PointingHandCursor);
    m_copyStatusBtn->setToolTip("Copy status message to clipboard");
    m_copyStatusBtn->setFixedSize(24, 20);
    m_copyStatusBtn->setStyleSheet(
        "QPushButton { color: #a0a0a0; border: none; padding: 0; font-size: 12px; background: transparent; }"
        "QPushButton:hover { color: #e0e0e0; background-color: #2a2a4a; border-radius: 3px; }"
    );
    m_copyStatusBtn->setVisible(false);
    connect(m_copyStatusBtn, &QPushButton::clicked, this, &MainWindow::onCopyStatusMessage);
    sb->addWidget(m_copyStatusBtn);

    // Recording elapsed time label (hidden by default)
    m_recordingTimeLabel = new QLabel(this);
    m_recordingTimeLabel->setStyleSheet("color: #e94560; font-size: 12px; font-weight: bold;");
    m_recordingTimeLabel->setVisible(false);
    sb->addWidget(m_recordingTimeLabel);

    // Show/hide copy button when status bar message changes
    connect(sb, &QStatusBar::messageChanged, this, &MainWindow::onStatusMessageChanged);

    // Platform info
    m_platformLabel = new QLabel(this);
    m_platformLabel->setStyleSheet("color: #a0a0a0; font-size: 12px;");
    try {
        PlatformInfo plat = AppState::instance().bridge().getPlatformInfo();
        m_platformLabel->setText(QString("%1/%2").arg(plat.os, plat.displayServer));
    } catch (...) {
        m_platformLabel->setText("Unknown platform");
    }
    sb->addPermanentWidget(m_platformLabel);
}

// ---------------------------------------------------------------------------
// Slots
// ---------------------------------------------------------------------------

void MainWindow::onSettingsClicked()
{
    // Settings dialog placeholder
    QMessageBox::information(this, "Settings", "Settings dialog coming soon.");
}

void MainWindow::onAbout()
{
    QMessageBox::about(this, "About Screen Dream",
        "Screen Dream v0.1.0\n\n"
        "A modern screen capture and recording application.\n"
        "Built with Qt and Rust.");
}

void MainWindow::onSourceSelected(const CaptureSource &source)
{
    delete m_selectedSource;
    m_selectedSource = new CaptureSource(source);
}

// ---------------------------------------------------------------------------
// Screenshot helpers
// ---------------------------------------------------------------------------

static QString screenshotOutputPath()
{
    QString dir = QDir::homePath() + "/Pictures";
    try {
        QJsonObject settings = AppState::instance().bridge().loadSettings();
        QJsonObject exp = settings.value("export").toObject();
        QString d = exp.value("output_directory").toString();
        if (!d.isEmpty()) dir = d;
    } catch (...) {}
    QDir().mkpath(dir);

    QString ts = QDateTime::currentDateTime().toString("yyyy-MM-dd_HH-mm-ss");
    return dir + "/screenshot_" + ts + ".png";
}

void MainWindow::onScreenScreenshot()
{
    // Use selected source if it's a screen, otherwise pick primary monitor
    CaptureSource src;
    if (m_selectedSource && m_selectedSource->type == CaptureSource::Screen) {
        src = *m_selectedSource;
    } else {
        // Default to first (primary) monitor
        try {
            auto sources = AppState::instance().bridge().enumerateSources();
            if (!sources.monitors.isEmpty()) {
                src.type = CaptureSource::Screen;
                src.monitorId = sources.monitors[0].id;
            }
        } catch (...) {}
    }

    try {
        QString path = AppState::instance().bridge().takeScreenshot(src, screenshotOutputPath());
        statusBar()->showMessage("Screenshot saved: " + path, 5000);
        m_recentCaptures->refresh();
    } catch (const std::exception &e) {
        statusBar()->showMessage(QString("Screenshot failed: %1").arg(e.what()), 5000);
    }
}

void MainWindow::onWindowScreenshot()
{
    if (!m_selectedSource || m_selectedSource->type != CaptureSource::Window) {
        statusBar()->showMessage("Select a window first from Browse Sources", 3000);
        return;
    }
    try {
        QString path = AppState::instance().bridge().takeScreenshot(*m_selectedSource, screenshotOutputPath());
        statusBar()->showMessage("Screenshot saved: " + path, 5000);
        m_recentCaptures->refresh();
    } catch (const std::exception &e) {
        statusBar()->showMessage(QString("Screenshot failed: %1").arg(e.what()), 5000);
    }
}

void MainWindow::onAreaScreenshot()
{
    if (!m_selectedSource || m_selectedSource->type != CaptureSource::Region) {
        statusBar()->showMessage("Select an area first from Browse Sources", 3000);
        return;
    }
    try {
        QString path = AppState::instance().bridge().takeScreenshot(*m_selectedSource, screenshotOutputPath());
        statusBar()->showMessage("Screenshot saved: " + path, 5000);
        m_recentCaptures->refresh();
    } catch (const std::exception &e) {
        statusBar()->showMessage(QString("Screenshot failed: %1").arg(e.what()), 5000);
    }
}

// ---------------------------------------------------------------------------
// Recording helpers
// ---------------------------------------------------------------------------

static QString recordingOutputPath()
{
    QString dir = QDir::homePath() + "/Pictures";
    try {
        QJsonObject settings = AppState::instance().bridge().loadSettings();
        QJsonObject exp = settings.value("export").toObject();
        QString d = exp.value("output_directory").toString();
        if (!d.isEmpty()) dir = d;
    } catch (...) {}
    QDir().mkpath(dir);

    QString ts = QDateTime::currentDateTime().toString("yyyy-MM-dd_HH-mm-ss");
    return dir + "/recording_" + ts + ".mp4";
}

void MainWindow::setAllCardsRecordingState(bool recording)
{
    m_screenCard->setRecordingState(recording);
    m_windowCard->setRecordingState(recording);
    m_areaCard->setRecordingState(recording);
}

void MainWindow::startRecording(const CaptureSource &source)
{
    RecordingConfig config;
    config.source = source;
    config.fps = 30;
    config.videoCodec = QStringLiteral("libx264");
    config.crf = 23;
    config.preset = QStringLiteral("ultrafast");
    config.outputPath = recordingOutputPath();
    config.captureMicrophone = false;

    try {
        AppState::instance().startRecording(config);
        m_isRecording = true;
        setAllCardsRecordingState(true);
        m_recordingElapsed.start();
        m_recordingTimeLabel->setText("REC 00:00");
        m_recordingTimeLabel->setVisible(true);
        m_recordingTimer->start();
        statusBar()->showMessage("Recording started...", 2000);
    } catch (const std::exception &e) {
        statusBar()->showMessage(QString("Recording failed: %1").arg(e.what()), 5000);
    }
}

void MainWindow::stopRecording()
{
    m_recordingTimer->stop();
    m_recordingTimeLabel->setVisible(false);

    try {
        QString path = AppState::instance().stopRecording();
        m_isRecording = false;
        setAllCardsRecordingState(false);
        statusBar()->showMessage("Recording saved: " + path, 8000);
        m_recentCaptures->refresh();
    } catch (const std::exception &e) {
        m_isRecording = false;
        setAllCardsRecordingState(false);
        statusBar()->showMessage(QString("Stop recording failed: %1").arg(e.what()), 5000);
    }
}

void MainWindow::onScreenRecord()
{
    if (m_isRecording) {
        stopRecording();
        return;
    }

    // Use selected source if it's a screen, otherwise pick primary monitor
    CaptureSource src;
    if (m_selectedSource && m_selectedSource->type == CaptureSource::Screen) {
        src = *m_selectedSource;
    } else {
        try {
            auto sources = AppState::instance().bridge().enumerateSources();
            if (!sources.monitors.isEmpty()) {
                src.type = CaptureSource::Screen;
                src.monitorId = sources.monitors[0].id;
            }
        } catch (...) {}
    }

    startRecording(src);
}

void MainWindow::onWindowRecord()
{
    if (m_isRecording) {
        stopRecording();
        return;
    }

    if (!m_selectedSource || m_selectedSource->type != CaptureSource::Window) {
        statusBar()->showMessage("Select a window first from Browse Sources", 3000);
        return;
    }

    startRecording(*m_selectedSource);
}

void MainWindow::onAreaRecord()
{
    if (m_isRecording) {
        stopRecording();
        return;
    }

    if (!m_selectedSource || m_selectedSource->type != CaptureSource::Region) {
        statusBar()->showMessage("Select an area first from Browse Sources", 3000);
        return;
    }

    startRecording(*m_selectedSource);
}

void MainWindow::onRecordingStateChanged(const RecordingStatus &status)
{
    if (status.state == RecordingStatus::Failed) {
        m_recordingTimer->stop();
        m_recordingTimeLabel->setVisible(false);
        m_isRecording = false;
        setAllCardsRecordingState(false);
        statusBar()->showMessage("Recording failed unexpectedly", 5000);
    }
}

void MainWindow::onRecordingTimerTick()
{
    qint64 ms = m_recordingElapsed.elapsed();
    int totalSecs = static_cast<int>(ms / 1000);
    int mins = totalSecs / 60;
    int secs = totalSecs % 60;
    m_recordingTimeLabel->setText(QString("REC %1:%2")
        .arg(mins, 2, 10, QChar('0'))
        .arg(secs, 2, 10, QChar('0')));
}

void MainWindow::onStatusMessageChanged(const QString &message)
{
    if (message.isEmpty()) {
        m_copyStatusBtn->setVisible(false);
        m_lastStatusMessage.clear();
    } else {
        m_lastStatusMessage = message;
        m_copyStatusBtn->setVisible(true);
    }
}

void MainWindow::onCopyStatusMessage()
{
    if (!m_lastStatusMessage.isEmpty()) {
        QApplication::clipboard()->setText(m_lastStatusMessage);
        // Brief visual feedback — temporarily change button text
        statusBar()->showMessage("Copied to clipboard!", 1500);
    }
}
