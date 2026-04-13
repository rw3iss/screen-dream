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
    /// Show a fullscreen overlay for region selection.
    /// backgroundImage: pre-captured screenshot used as frozen background.
    /// outputPath: where to save the cropped result on Enter.
    explicit RegionPicker(const QPixmap &backgroundImage,
                          const QString &outputPath,
                          QWidget *parent = nullptr);

signals:
    void regionCaptured(const QString &savedPath);
    void cancelled();

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;

private:
    void cropAndSave();

    QPixmap m_background;
    QString m_outputPath;
    qreal m_scale = 1.0;   // physical / logical scale
    bool m_selecting = false;
    bool m_hasSelection = false;
    QPoint m_startPos;
    QPoint m_endPos;
    QRect m_selection;
};

#endif // REGIONPICKER_H
