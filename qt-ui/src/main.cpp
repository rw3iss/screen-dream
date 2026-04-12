#include <QApplication>
#include <QDebug>
#include <QStandardPaths>
#include <QDir>
#include <QFile>
#include <QSystemTrayIcon>
#include <QMenu>
#include <QAction>

#include "core/AppState.h"
#include "ui/MainWindow.h"

static QString loadStyleSheet()
{
    QFile qss(":/styles/dark-theme.qss");
    if (qss.open(QIODevice::ReadOnly | QIODevice::Text)) {
        return QString::fromUtf8(qss.readAll());
    }
    qWarning() << "Could not load dark-theme.qss from resources";
    return {};
}

int main(int argc, char *argv[])
{
    QApplication app(argc, argv);
    app.setApplicationName("Screen Dream");
    app.setApplicationVersion("0.1.0");
    app.setOrganizationName("ScreenDream");

    // Load dark theme
    QString style = loadStyleSheet();
    if (!style.isEmpty()) {
        app.setStyleSheet(style);
    }

    // Config directory
    QString configDir = QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation);
    if (configDir.isEmpty()) {
        configDir = QDir::homePath() + "/.config/screen-dream";
    }

    try {
        AppState::init(configDir);
    } catch (const std::exception &e) {
        qCritical() << "Failed to initialize:" << e.what();
        return 1;
    }

    AppState &state = AppState::instance();

    // Print platform info
    PlatformInfo plat = state.bridge().getPlatformInfo();
    qInfo() << "Platform:" << plat.os
            << "| Display:" << plat.displayServer
            << "| Arch:" << plat.arch;

    // Print FFmpeg status
    try {
        FfmpegStatus ffmpeg = state.bridge().getFfmpegStatus();
        qInfo() << "FFmpeg available:" << ffmpeg.available;
        if (ffmpeg.available) {
            qInfo() << "  Version:" << ffmpeg.version;
        }
    } catch (const std::exception &e) {
        qWarning() << "FFmpeg status error:" << e.what();
    }

    // Create and show main window
    MainWindow mainWindow;
    mainWindow.show();

    // System tray icon
    QSystemTrayIcon trayIcon;
    trayIcon.setToolTip("Screen Dream");

    QMenu trayMenu;
    QAction *showAction = trayMenu.addAction("Show");
    QObject::connect(showAction, &QAction::triggered, &mainWindow, &QMainWindow::show);

    trayMenu.addSeparator();

    QAction *screenshotAction = trayMenu.addAction("Screenshot");
    Q_UNUSED(screenshotAction);  // TODO: connect to capture logic

    QAction *recordAction = trayMenu.addAction("Start Recording");
    Q_UNUSED(recordAction);  // TODO: connect to recording logic

    trayMenu.addSeparator();

    QAction *quitAction = trayMenu.addAction("Quit");
    QObject::connect(quitAction, &QAction::triggered, &app, &QApplication::quit);

    trayIcon.setContextMenu(&trayMenu);

    // Only show tray icon if system supports it
    if (QSystemTrayIcon::isSystemTrayAvailable()) {
        trayIcon.show();
    }

    return app.exec();
}
