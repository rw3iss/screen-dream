#include "widgets/RegionPicker.h"

#include <QPainter>
#include <QPainterPath>
#include <QMouseEvent>
#include <QKeyEvent>
#include <QGuiApplication>
#include <QScreen>
#include <QDir>
#include <QFileInfo>

RegionPicker::RegionPicker(const QPixmap &backgroundImage,
                           const QString &outputPath,
                           QWidget *parent)
    : QWidget(parent), m_background(backgroundImage), m_outputPath(outputPath)
{
    setWindowFlags(Qt::FramelessWindowHint
                 | Qt::WindowStaysOnTopHint
                 | Qt::X11BypassWindowManagerHint);
    setAttribute(Qt::WA_DeleteOnClose);
    setCursor(Qt::CrossCursor);
    setMouseTracking(true);

    // Cover all monitors (virtual desktop bounding box in logical coords)
    QRect virtualGeo;
    for (const QScreen *s : QGuiApplication::screens())
        virtualGeo = virtualGeo.united(s->geometry());
    setGeometry(virtualGeo);

    // Compute scale: physical (image) / logical (overlay)
    if (virtualGeo.width() > 0 && !m_background.isNull())
        m_scale = (qreal)m_background.width() / virtualGeo.width();

    // Scale the background pixmap down to logical size for display
    if (!m_background.isNull())
        m_background = m_background.scaled(virtualGeo.size(),
                                           Qt::IgnoreAspectRatio,
                                           Qt::SmoothTransformation);

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

    // Draw the frozen desktop as background
    if (!m_background.isNull())
        p.drawPixmap(0, 0, m_background);
    else
        p.fillRect(rect(), QColor(30, 30, 50));

    // Semi-transparent dark overlay everywhere except the selection
    QColor overlay(0, 0, 0, 100);

    if (m_hasSelection && !m_selection.isEmpty()) {
        // Darken everything outside the selection
        QPainterPath fullPath;
        fullPath.addRect(QRectF(rect()));
        QPainterPath selPath;
        selPath.addRect(QRectF(m_selection));
        p.fillPath(fullPath.subtracted(selPath), overlay);

        // Selection border
        p.setPen(QPen(QColor("#e94560"), 2));
        p.setBrush(Qt::NoBrush);
        p.drawRect(m_selection);

        // Dimension label
        int sw = qRound(m_selection.width() * m_scale);
        int sh = qRound(m_selection.height() * m_scale);
        QString label = QString("%1 x %2").arg(sw).arg(sh);
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

        // Hint
        if (!m_selecting) {
            QString hint = "Enter to capture, Escape to cancel";
            int hw = fm.horizontalAdvance(hint) + 16;
            int hx = width() / 2 - hw / 2;
            p.setPen(Qt::NoPen);
            p.setBrush(QColor(0, 0, 0, 180));
            p.drawRoundedRect(hx, 20, hw, labelH, 4, 4);
            p.setPen(Qt::white);
            p.drawText(hx + 8, 20 + fm.ascent() + 3, hint);
        }
    } else {
        p.fillRect(rect(), overlay);
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
        cropAndSave();
    }
}

void RegionPicker::cropAndSave()
{
    // The selection is in logical overlay coordinates.
    // The original (pre-scaled) background image is at physical resolution.
    // We need to reload the original to crop at full quality.
    // But we don't have the original anymore — we scaled it.
    // So we crop from the original file that MainWindow captured.

    // Actually, MainWindow passes the original pixmap and we scaled it for display.
    // We need to crop from the ORIGINAL. Let's use the output path's temp file.
    // MainWindow saved the Spectacle screenshot to a temp path.
    // We'll have MainWindow handle the crop after we emit the selection.

    // Convert logical selection to physical coordinates
    QRect overlayGeo = geometry();
    int desktopX = overlayGeo.x() + m_selection.x();
    int desktopY = overlayGeo.y() + m_selection.y();

    QRect physicalRect(
        qRound(desktopX * m_scale),
        qRound(desktopY * m_scale),
        qRound(m_selection.width() * m_scale),
        qRound(m_selection.height() * m_scale)
    );

    fprintf(stderr, "RegionPicker: scale=%.2f selection=(%d,%d %dx%d) -> physical=(%d,%d %dx%d)\n",
            m_scale,
            m_selection.x(), m_selection.y(), m_selection.width(), m_selection.height(),
            physicalRect.x(), physicalRect.y(), physicalRect.width(), physicalRect.height());

    emit regionCaptured(QString("%1,%2,%3,%4")
        .arg(physicalRect.x()).arg(physicalRect.y())
        .arg(physicalRect.width()).arg(physicalRect.height()));
    close();
}
