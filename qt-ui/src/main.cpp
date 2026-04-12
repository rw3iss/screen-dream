#include <QApplication>
#include <QDebug>
#include <QStandardPaths>
#include <QDir>

#include "core/AppState.h"

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    app.setApplicationName("ScreenDream");
    app.setApplicationVersion("0.1.0");
    app.setOrganizationName("ScreenDream");

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
            qInfo() << "  Source:" << ffmpeg.sourceDescription;
            qInfo() << "  Video encoders:" << ffmpeg.videoEncoders.join(", ");
            qInfo() << "  Audio encoders:" << ffmpeg.audioEncoders.join(", ");
        }
    } catch (const std::exception &e) {
        qWarning() << "FFmpeg status error:" << e.what();
    }

    qInfo() << "Screen Dream Qt UI initialized. No window yet -- exiting.";

    // For now, exit cleanly without entering the event loop (no window).
    return 0;
}
