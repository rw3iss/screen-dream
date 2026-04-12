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
#include <QApplication>
#include <QJsonObject>
#include <QFrame>

MainWindow::MainWindow(QWidget *parent)
    : QMainWindow(parent), m_selectedSource(nullptr)
{
    setWindowTitle("Screen Dream");
    resize(800, 600);
    setMinimumSize(640, 480);

    setupMenuBar();
    setupCentralWidget();
    setupStatusBar();
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
    mainLayout->setContentsMargins(16, 16, 16, 16);
    mainLayout->setSpacing(16);

    // --- Capture cards row (fixed height, no stretch) ---
    auto *cardsLayout = new QHBoxLayout();
    cardsLayout->setAlignment(Qt::AlignCenter);
    cardsLayout->setSpacing(20);

    m_screenCard = new CaptureCard(CaptureCard::Screen, central);
    m_windowCard = new CaptureCard(CaptureCard::Window, central);
    m_areaCard   = new CaptureCard(CaptureCard::Area, central);

    cardsLayout->addWidget(m_screenCard);
    cardsLayout->addWidget(m_windowCard);
    cardsLayout->addWidget(m_areaCard);

    mainLayout->addLayout(cardsLayout, 0);  // no stretch — fixed height

    // --- Source browser (collapsible, takes all remaining space) ---
    m_sourceBrowser = new SourceBrowser(central);
    m_sourceBrowser->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
    mainLayout->addWidget(m_sourceBrowser, 1);  // stretch=1 — fills remaining height
    connect(m_sourceBrowser, &SourceBrowser::sourceSelected,
            this, &MainWindow::onSourceSelected);

    // --- Separator with label ---
    auto *sepLayout = new QHBoxLayout();
    sepLayout->setSpacing(8);

    auto *leftLine = new QFrame(central);
    leftLine->setFrameShape(QFrame::HLine);
    leftLine->setStyleSheet("color: #2a2a4a;");

    auto *sepLabel = new QLabel("Recent Captures", central);
    sepLabel->setStyleSheet("color: #a0a0a0; font-size: 13px; background-color: transparent;");

    auto *rightLine = new QFrame(central);
    rightLine->setFrameShape(QFrame::HLine);
    rightLine->setStyleSheet("color: #2a2a4a;");

    sepLayout->addWidget(leftLine, 1);
    sepLayout->addWidget(sepLabel, 0);
    sepLayout->addWidget(rightLine, 1);

    mainLayout->addLayout(sepLayout, 0);  // no stretch

    // --- Recent captures (fixed height band at bottom) ---
    m_recentCaptures = new RecentCaptures(central);
    m_recentCaptures->setFixedHeight(140);
    m_recentCaptures->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    mainLayout->addWidget(m_recentCaptures, 0);  // no stretch — stays at bottom

    // Load output directory from settings
    try {
        QJsonObject settings = AppState::instance().bridge().loadSettings();
        QString outputDir = settings.value("output_directory").toString();
        if (outputDir.isEmpty()) outputDir = "/tmp";
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
