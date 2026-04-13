#include "widgets/RegionPicker.h"

#include <QPainter>
#include <QMouseEvent>
#include <QKeyEvent>
#include <QGuiApplication>
#include <QScreen>
#include <QDir>
#include <QCoreApplication>
#include <QThread>
#include <QFileInfo>
#include <QUrl>
#include <QTimer>
#include <QEventLoop>
#include <QProcess>
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
    // Hide overlay so it doesn't appear in the screenshot
    hide();
    QCoreApplication::processEvents();
    QThread::msleep(100); // Brief wait for compositor to remove our window

    // Use Spectacle CLI for a synchronous full-screen capture.
    // -b = background (no GUI), -n = no notification, -f = full screen, -o = output path
    fprintf(stderr, "RegionPicker: capturing full screen via spectacle CLI...\n");
    QString tempPath = QStringLiteral("/tmp/sd_region_bg.png");
    QFile::remove(tempPath); // ensure clean state
    QProcess proc;
    proc.start(QStringLiteral("spectacle"), {"-b", "-n", "-f", "-o", tempPath});
    if (!proc.waitForFinished(5000)) {
        qWarning() << "Spectacle timed out";
        emit cancelled();
        close();
        return;
    }

    if (proc.exitCode() != 0) {
        qWarning() << "Spectacle failed:" << proc.readAllStandardError();
        emit cancelled();
        close();
        return;
    }

    // Load the full-screen image
    QImage fullImg(tempPath);
    if (fullImg.isNull()) {
        qWarning() << "Failed to load Spectacle screenshot:" << tempPath;
        QFile::remove(tempPath);
        emit cancelled();
        close();
        return;
    }

    // The overlay covers ONE monitor (the one where it was shown) in logical pixels.
    // The Spectacle image covers ALL monitors at physical (native) resolution.
    //
    // To map selection→physical: we need to know which monitor the overlay is on
    // and its physical position in the Spectacle image.
    QRect overlayGeo = geometry(); // overlay geometry in logical desktop coords

    // Find which screen the overlay center is on
    QScreen *overlayScreen = nullptr;
    QPoint overlayCenter = overlayGeo.center();
    for (QScreen *s : QGuiApplication::screens()) {
        if (s->geometry().contains(overlayCenter)) {
            overlayScreen = s;
            break;
        }
    }
    if (!overlayScreen) overlayScreen = QGuiApplication::primaryScreen();

    // Compute scale from the full virtual desktop size vs Spectacle image size.
    // devicePixelRatio() is unreliable on Wayland (returns 2.0 when actual is 1.5).
    QRect virtualGeo;
    for (QScreen *s : QGuiApplication::screens())
        virtualGeo = virtualGeo.united(s->geometry());

    qreal scale = 1.5; // fallback
    if (virtualGeo.width() > 0)
        scale = (qreal)fullImg.width() / virtualGeo.width();

    // The overlay's top-left is at overlayGeo.topLeft() in logical desktop coords.
    // Selection is relative to the overlay widget, so in desktop coords:
    //   desktop_x = overlayGeo.x() + selection.x()
    //   desktop_y = overlayGeo.y() + selection.y()
    // Physical coords in Spectacle image:
    //   phys_x = desktop_x * scale
    //   phys_y = desktop_y * scale
    int desktopX = overlayGeo.x() + m_selection.x();
    int desktopY = overlayGeo.y() + m_selection.y();

    int cx = qRound(desktopX * scale);
    int cy = qRound(desktopY * scale);
    int cw = qRound(m_selection.width() * scale);
    int ch = qRound(m_selection.height() * scale);

    fprintf(stderr, "RegionPicker: overlay=(%d,%d %dx%d) screen=%s scale=%.2f\n"
                    "  selection=(%d,%d %dx%d) desktop=(%d,%d) physical=(%d,%d %dx%d)\n"
                    "  image=%dx%d\n",
            overlayGeo.x(), overlayGeo.y(), overlayGeo.width(), overlayGeo.height(),
            overlayScreen ? overlayScreen->name().toUtf8().constData() : "?", scale,
            m_selection.x(), m_selection.y(), m_selection.width(), m_selection.height(),
            desktopX, desktopY, cx, cy, cw, ch,
            fullImg.width(), fullImg.height());

    // Clamp to image bounds
    cx = qMax(0, qMin(cx, fullImg.width() - 1));
    cy = qMax(0, qMin(cy, fullImg.height() - 1));
    cw = qMin(cw, fullImg.width() - cx);
    ch = qMin(ch, fullImg.height() - cy);

    // Crop with QImage (instant)
    QImage cropped = fullImg.copy(cx, cy, cw, ch);

    // Save directly
    QDir().mkpath(QFileInfo(m_outputPath).absolutePath());
    bool saved = cropped.save(m_outputPath, "PNG");

    // Cleanup temp file
    QFile::remove(tempPath);

    if (saved) {
        fprintf(stderr, "RegionPicker: saved %dx%d crop=(%d,%d %dx%d) -> %s\n",
                cropped.width(), cropped.height(), cx, cy, cw, ch, m_outputPath.toUtf8().constData());
        emit regionCaptured(m_outputPath);
    } else {
        fprintf(stderr, "RegionPicker: FAILED to save cropped screenshot\n");
        emit cancelled();
    }
    close();
}
