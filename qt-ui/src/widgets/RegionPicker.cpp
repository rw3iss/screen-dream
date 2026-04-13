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
#include <QDBusInterface>
#include <QDBusReply>
#include <QDBusConnection>
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
    QThread::msleep(50); // Brief wait for compositor to remove our window

    // Use the portal Screenshot API with interactive=false — fast, silent, native resolution.
    // Call via QDBus directly — no Rust, no Spectacle, no helper.
    QDBusInterface portal("org.freedesktop.portal.Desktop",
                          "/org/freedesktop/portal/desktop",
                          "org.freedesktop.portal.Screenshot",
                          QDBusConnection::sessionBus());

    QVariantMap options;
    options["interactive"] = false;

    QDBusReply<QDBusObjectPath> reply = portal.call("Screenshot", QString(), options);
    if (!reply.isValid()) {
        qWarning() << "Portal Screenshot call failed:" << reply.error().message();
        emit cancelled();
        close();
        return;
    }

    QString requestPath = reply.value().path();

    // Wait for the Response signal by polling the portal's screenshot output.
    // The portal saves to ~/Pictures/Screenshots/ — find the newest file.
    QDir screenshotDir(QDir::homePath() + "/Pictures/Screenshots");
    QStringList beforeFiles = screenshotDir.entryList({"Screenshot_*.png"}, QDir::Files, QDir::Time);
    QString beforeLatest = beforeFiles.isEmpty() ? QString() : beforeFiles.first();

    // Give the portal time to process
    bool success = false;
    QString capturedUri;
    for (int i = 0; i < 50; i++) { // up to 5 seconds
        QThread::msleep(100);
        QCoreApplication::processEvents();
        QStringList afterFiles = screenshotDir.entryList({"Screenshot_*.png"}, QDir::Files, QDir::Time);
        if (!afterFiles.isEmpty() && afterFiles.first() != beforeLatest) {
            capturedUri = screenshotDir.absoluteFilePath(afterFiles.first());
            success = true;
            break;
        }
    }

    if (!success || capturedUri.isEmpty()) {
        qWarning() << "Portal screenshot failed or timed out";
        emit cancelled();
        close();
        return;
    }

    QString fullPath = capturedUri;
    if (!QFile::exists(fullPath)) {
        qWarning() << "Portal screenshot file not found:" << fullPath;
        emit cancelled();
        close();
        return;
    }

    // Load the image with Qt (fast — QImage loads PNGs quickly)
    QImage fullImg(fullPath);
    if (fullImg.isNull()) {
        qWarning() << "Failed to load screenshot:" << fullPath;
        QFile::remove(fullPath);
        emit cancelled();
        close();
        return;
    }

    // Scale selection coordinates: our overlay is in logical pixels,
    // the portal screenshot is in physical pixels.
    QScreen *screen = QGuiApplication::primaryScreen();
    qreal scale = screen ? screen->devicePixelRatio() : 1.0;

    int cx = qRound(m_selection.x() * scale);
    int cy = qRound(m_selection.y() * scale);
    int cw = qRound(m_selection.width() * scale);
    int ch = qRound(m_selection.height() * scale);

    // Clamp to image bounds
    cx = qMax(0, qMin(cx, fullImg.width() - 1));
    cy = qMax(0, qMin(cy, fullImg.height() - 1));
    cw = qMin(cw, fullImg.width() - cx);
    ch = qMin(ch, fullImg.height() - cy);

    // Crop with QImage (instant — no FFmpeg needed)
    QImage cropped = fullImg.copy(cx, cy, cw, ch);

    // Save directly
    QDir().mkpath(QFileInfo(m_outputPath).absolutePath());
    bool saved = cropped.save(m_outputPath, "PNG");

    // Cleanup the portal's temp file
    QFile::remove(fullPath);

    if (saved) {
        qDebug() << "Region screenshot saved:" << cropped.size() << "->" << m_outputPath;
        emit regionCaptured(m_outputPath);
    } else {
        qWarning() << "Failed to save cropped screenshot";
        emit cancelled();
    }
    close();
}
