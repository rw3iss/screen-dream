#include "widgets/RegionPicker.h"

#include <QPainter>
#include <QPainterPath>
#include <QMouseEvent>
#include <QKeyEvent>
#include <QGuiApplication>
#include <QScreen>
#include <QColor>
#include <QFont>
#include <QFontMetrics>
#include <QCursor>
#include <QRegion>

RegionPicker::RegionPicker(const QString &backgroundPath, QWidget *parent)
    : QWidget(parent)
{
    // Frameless, always-on-top, bypass WM for reliable full-coverage
    setWindowFlags(Qt::FramelessWindowHint
                 | Qt::WindowStaysOnTopHint
                 | Qt::X11BypassWindowManagerHint);
    setAttribute(Qt::WA_DeleteOnClose);
    setAttribute(Qt::WA_TranslucentBackground, false);
    setCursor(Qt::CrossCursor);
    setMouseTracking(true);

    // Load the pre-captured desktop screenshot
    m_background.load(backgroundPath);

    // Compute bounding box that covers all monitors
    QRect virtualGeo;
    const auto screens = QGuiApplication::screens();
    for (const QScreen *s : screens) {
        virtualGeo = virtualGeo.united(s->geometry());
    }
    setGeometry(virtualGeo);

    // Scale background to match the virtual desktop if needed
    if (!m_background.isNull() && m_background.size() != virtualGeo.size()) {
        m_background = m_background.scaled(virtualGeo.size(),
                                           Qt::IgnoreAspectRatio,
                                           Qt::SmoothTransformation);
    }

    setFocusPolicy(Qt::StrongFocus);
    showFullScreen();
    activateWindow();
    raise();
    setFocus();
}

// ---------------------------------------------------------------------------
// Painting
// ---------------------------------------------------------------------------

void RegionPicker::paintEvent(QPaintEvent * /*event*/)
{
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing, false);

    // 1. Draw the desktop background
    if (!m_background.isNull()) {
        p.drawPixmap(0, 0, m_background);
    } else {
        p.fillRect(rect(), Qt::black);
    }

    // 2. Semi-transparent dark overlay on top of the whole surface
    QColor overlay(0, 0, 0, 25);  // ~10 % black

    if (m_hasSelection && !m_selection.isEmpty()) {
        // Draw overlay everywhere EXCEPT the selection
        QPainterPath fullPath;
        fullPath.addRect(QRectF(rect()));
        QPainterPath selPath;
        selPath.addRect(QRectF(m_selection));
        QPainterPath darkPath = fullPath.subtracted(selPath);
        p.fillPath(darkPath, overlay);

        // 3. Selection border
        QPen pen(QColor("#e94560"), 2);
        p.setPen(pen);
        p.setBrush(Qt::NoBrush);
        p.drawRect(m_selection);

        // 4. Dimension label
        int w = m_selection.width();
        int h = m_selection.height();
        QString label = QString("%1x%2").arg(w).arg(h);

        QFont font = p.font();
        font.setPixelSize(14);
        font.setBold(true);
        p.setFont(font);

        QFontMetrics fm(font);
        QRect textRect = fm.boundingRect(label);
        int labelX = m_selection.center().x() - textRect.width() / 2;
        int labelY = m_selection.bottom() + 22;
        // Keep label on-screen
        if (labelY + textRect.height() > height())
            labelY = m_selection.top() - 8;

        // Background pill for readability
        QRect pill(labelX - 6, labelY - textRect.height() - 2,
                   textRect.width() + 12, textRect.height() + 6);
        p.setPen(Qt::NoPen);
        p.setBrush(QColor(0, 0, 0, 180));
        p.drawRoundedRect(pill, 4, 4);

        p.setPen(Qt::white);
        p.drawText(labelX, labelY, label);
    } else {
        // No selection yet — dim the entire surface
        p.fillRect(rect(), overlay);
    }
}

// ---------------------------------------------------------------------------
// Mouse events
// ---------------------------------------------------------------------------

void RegionPicker::mousePressEvent(QMouseEvent *event)
{
    if (event->button() == Qt::LeftButton) {
        m_origin = event->pos();
        m_dragging = true;
        m_hasSelection = false;
        m_selection = QRect();
        update();
    }
}

void RegionPicker::mouseMoveEvent(QMouseEvent *event)
{
    if (m_dragging) {
        m_selection = QRect(m_origin, event->pos()).normalized();
        m_hasSelection = true;
        update();
    }
}

void RegionPicker::mouseReleaseEvent(QMouseEvent *event)
{
    if (event->button() == Qt::LeftButton && m_dragging) {
        m_dragging = false;
        m_selection = QRect(m_origin, event->pos()).normalized();
        m_hasSelection = !m_selection.isEmpty();
        update();
    }
}

// ---------------------------------------------------------------------------
// Keyboard
// ---------------------------------------------------------------------------

void RegionPicker::keyPressEvent(QKeyEvent *event)
{
    if (event->key() == Qt::Key_Escape) {
        close();
    } else if ((event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter)
               && m_hasSelection && !m_selection.isEmpty()) {
        emit regionSelected(m_selection);
        close();
    }
}
