#ifndef REGIONPICKER_H
#define REGIONPICKER_H

#include <QWidget>
#include <QRect>
#include <QPoint>
#include <QString>

class RegionPicker : public QWidget {
    Q_OBJECT

public:
    /// Show a fullscreen semi-transparent overlay for region selection.
    /// outputPath: where to save the cropped screenshot on Enter.
    explicit RegionPicker(const QString &outputPath, QWidget *parent = nullptr);

signals:
    /// Emitted after the user confirms and the screenshot is saved.
    void regionCaptured(const QString &savedPath);
    /// Emitted if the user presses Escape.
    void cancelled();

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;

private:
    void captureAndSave();

    QString m_outputPath;
    bool m_selecting = false;
    bool m_hasSelection = false;
    QPoint m_startPos;
    QPoint m_endPos;
    QRect m_selection;
};

#endif // REGIONPICKER_H
