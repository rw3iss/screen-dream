#ifndef REGIONPICKER_H
#define REGIONPICKER_H

#include <QWidget>
#include <QPixmap>
#include <QRect>
#include <QPoint>
#include <QString>

class RegionPicker : public QWidget {
    Q_OBJECT

public:
    /// Construct the picker. \a backgroundPath is the path to a full-desktop
    /// screenshot that will be painted behind the overlay.
    explicit RegionPicker(const QString &backgroundPath, QWidget *parent = nullptr);

signals:
    /// Emitted when the user presses Enter to confirm a selection.
    void regionSelected(QRect region);

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;

private:
    QPixmap m_background;      ///< Full-desktop screenshot used as background
    QPoint  m_origin;          ///< Mouse-press start point
    QRect   m_selection;       ///< Current selection rectangle (normalised)
    bool    m_dragging = false;
    bool    m_hasSelection = false;
};

#endif // REGIONPICKER_H
