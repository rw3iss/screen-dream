#include "widgets/RegionPicker.h"

#include <QPainter>
#include <QMouseEvent>
#include <QKeyEvent>
#include <QGuiApplication>
#include <QScreen>
#include <QProcess>
#include <QDir>
#include <QCoreApplication>
#include <QThread>
#include <QFileInfo>
#include <QDebug>

RegionPicker::RegionPicker(const QString &outputPath, QWidget *parent)
    : QWidget(parent), m_outputPath(outputPath)
{
    // Frameless, always-on-top, bypass WM so it covers everything reliably
    setWindowFlags(Qt::FramelessWindowHint
                 | Qt::WindowStaysOnTopHint
                 | Qt::X11BypassWindowManagerHint);
    setAttribute(Qt::WA_DeleteOnClose);
    // NO translucent background — we paint our own solid semi-opaque fill
    setAttribute(Qt::WA_TranslucentBackground, false);
    setCursor(Qt::CrossCursor);
    setMouseTracking(true);

    // Cover all monitors
    QRect virtualGeo;
    const auto screens = QGuiApplication::screens();
    for (const QScreen *s : screens)
        virtualGeo = virtualGeo.united(s->geometry());
    setGeometry(virtualGeo);

    setFocusPolicy(Qt::StrongFocus);
    showFullScreen();
    activateWindow();
    raise();
    setFocus();
}

void RegionPicker::paintEvent(QPaintEvent *)
{
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing, false);

    // Dark semi-opaque background — simple solid fill, very fast
    p.fillRect(rect(), QColor(0, 0, 0, 80)); // ~30% black

    if (m_hasSelection && !m_selection.isEmpty()) {
        // Clear the selected region (punch a hole through the overlay)
        p.setCompositionMode(QPainter::CompositionMode_Clear);
        p.fillRect(m_selection, Qt::transparent);
        p.setCompositionMode(QPainter::CompositionMode_SourceOver);

        // Selection border
        p.setPen(QPen(QColor("#e94560"), 2));
        p.setBrush(Qt::NoBrush);
        p.drawRect(m_selection);

        // Dimension label
        QString label = QString("%1 x %2").arg(m_selection.width()).arg(m_selection.height());
        QFont font = p.font();
        font.setPixelSize(13);
        font.setBold(true);
        p.setFont(font);

        QFontMetrics fm(font);
        int labelW = fm.horizontalAdvance(label) + 12;
        int labelH = fm.height() + 6;
        int labelX = m_selection.center().x() - labelW / 2;
        int labelY = m_selection.bottom() + 8;
        if (labelY + labelH > height())
            labelY = m_selection.top() - labelH - 4;

        p.setPen(Qt::NoPen);
        p.setBrush(QColor(0, 0, 0, 200));
        p.drawRoundedRect(labelX, labelY, labelW, labelH, 4, 4);
        p.setPen(Qt::white);
        p.drawText(labelX + 6, labelY + fm.ascent() + 3, label);

        // Instruction
        if (!m_selecting) {
            QString hint = "Press Enter to capture, Escape to cancel";
            int hintW = fm.horizontalAdvance(hint) + 16;
            int hx = width() / 2 - hintW / 2;
            int hy = 20;
            p.setPen(Qt::NoPen);
            p.setBrush(QColor(0, 0, 0, 180));
            p.drawRoundedRect(hx, hy, hintW, labelH, 4, 4);
            p.setPen(Qt::white);
            p.drawText(hx + 8, hy + fm.ascent() + 3, hint);
        }
    } else {
        // No selection yet — show instruction
        QFont font = p.font();
        font.setPixelSize(16);
        p.setFont(font);
        p.setPen(QColor(255, 255, 255, 200));
        p.drawText(rect(), Qt::AlignCenter, "Click and drag to select a region");
    }
}

void RegionPicker::mousePressEvent(QMouseEvent *event)
{
    if (event->button() == Qt::LeftButton) {
        m_startPos = event->pos();
        m_selecting = true;
        m_hasSelection = false;
        m_selection = QRect();
        update();
    }
}

void RegionPicker::mouseMoveEvent(QMouseEvent *event)
{
    if (m_selecting) {
        m_endPos = event->pos();
        m_selection = QRect(m_startPos, m_endPos).normalized();
        m_hasSelection = !m_selection.isEmpty();
        update();
    }
}

void RegionPicker::mouseReleaseEvent(QMouseEvent *event)
{
    if (event->button() == Qt::LeftButton && m_selecting) {
        m_selecting = false;
        m_endPos = event->pos();
        m_selection = QRect(m_startPos, m_endPos).normalized();
        m_hasSelection = m_selection.width() > 5 && m_selection.height() > 5;
        update();
    }
}

void RegionPicker::keyPressEvent(QKeyEvent *event)
{
    if (event->key() == Qt::Key_Escape) {
        emit cancelled();
        close();
    } else if ((event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter)
               && m_hasSelection) {
        captureAndSave();
    }
}

void RegionPicker::captureAndSave()
{
    // Hide overlay so it's not in the screenshot
    hide();
    // Brief delay for the overlay to disappear from the compositor
    QCoreApplication::processEvents();
    QThread::msleep(100);

    // Find the DRM helper next to the executable
    QString helperPath = QCoreApplication::applicationDirPath() + "/drm_capture_helper";
    bool useDrm = QFile::exists(helperPath);

    if (useDrm) {
        // Use DRM helper: list planes to find the right one, then capture
        // For simplicity, capture via Spectacle-less path:
        // 1. Find which plane covers our selection
        // 2. Capture that plane
        // 3. Crop with FFmpeg

        // List active planes
        QProcess listProc;
        listProc.start(helperPath, {"--list", "/dev/dri/card2"});
        listProc.waitForFinished(3000);
        QString listOutput = listProc.readAllStandardOutput();

        // Parse first plane (primary monitor)
        // Format: PLANE:51:CRTC:62:FB:153:SIZE:3840x2160:POS:0,0:FMT:AB4H
        uint32_t planeId = 0;
        for (const QString &line : listOutput.split('\n')) {
            if (line.startsWith("PLANE:")) {
                QStringList parts = line.split(':');
                if (parts.size() >= 2) {
                    planeId = parts[1].toUInt();
                    break; // Use first plane for now
                }
            }
        }

        if (planeId > 0) {
            // Capture the plane to a temp file
            QString tmpRaw = QDir::tempPath() + "/sd_drm_raw.data";
            QProcess capProc;
            capProc.start(helperPath, {"/dev/dri/card2", QString::number(planeId)});
            capProc.waitForFinished(5000);
            QByteArray rawData = capProc.readAllStandardOutput();

            if (rawData.size() > 12) {
                // Parse header
                uint32_t w, h, pitch;
                memcpy(&w, rawData.data(), 4);
                memcpy(&h, rawData.data() + 4, 4);
                memcpy(&pitch, rawData.data() + 8, 4);

                qDebug() << "DRM capture:" << w << "x" << h << "pitch:" << pitch
                         << "data:" << rawData.size() - 12 << "bytes";

                // TODO: pixel format conversion and crop
                // For now, fall through to Spectacle
            }
        }
    }

    // Fallback: use Spectacle for the actual capture, FFmpeg for crop
    // This is simple and reliable
    QProcess spectacle;
    spectacle.start("spectacle", {"-b", "-n", "-f", "-o",
                     QDir::tempPath() + "/sd_region_full.png"});
    spectacle.waitForFinished(5000);

    // Wait for file to appear (Spectacle is async)
    QString fullPath = QDir::tempPath() + "/sd_region_full.png";
    for (int i = 0; i < 30; i++) {
        if (QFile::exists(fullPath) && QFileInfo(fullPath).size() > 0)
            break;
        QThread::msleep(100);
    }

    if (!QFile::exists(fullPath)) {
        qWarning() << "Spectacle did not produce output";
        emit cancelled();
        close();
        return;
    }

    // Get the image dimensions to calculate scale factor
    // Our selection is in logical screen coordinates
    // Spectacle captures at physical resolution
    // Scale factor = physical / logical
    QScreen *primaryScreen = QGuiApplication::primaryScreen();
    qreal scale = primaryScreen ? primaryScreen->devicePixelRatio() : 1.0;

    int cx = qRound(m_selection.x() * scale);
    int cy = qRound(m_selection.y() * scale);
    int cw = qRound(m_selection.width() * scale);
    int ch = qRound(m_selection.height() * scale);

    // Crop with FFmpeg
    QProcess ffmpeg;
    QString cropFilter = QString("crop=%1:%2:%3:%4").arg(cw).arg(ch).arg(cx).arg(cy);
    ffmpeg.start("ffmpeg", {"-y", "-i", fullPath, "-vf", cropFilter,
                            "-frames:v", "1", "-update", "1", m_outputPath});
    ffmpeg.waitForFinished(10000);

    // Cleanup
    QFile::remove(fullPath);

    if (QFile::exists(m_outputPath) && QFileInfo(m_outputPath).size() > 0) {
        emit regionCaptured(m_outputPath);
    } else {
        qWarning() << "FFmpeg crop failed";
        emit cancelled();
    }
    close();
}
