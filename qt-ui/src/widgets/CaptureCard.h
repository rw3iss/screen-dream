#ifndef CAPTURECARD_H
#define CAPTURECARD_H

#include <QWidget>
#include <QLabel>
#include <QPushButton>
#include <QVBoxLayout>

class CaptureCard : public QWidget {
    Q_OBJECT

public:
    enum CaptureType { Screen, Window, Area };

    explicit CaptureCard(CaptureType type, QWidget *parent = nullptr);

signals:
    void screenshotClicked();
    void recordClicked();

protected:
    void enterEvent(QEnterEvent *event) override;
    void leaveEvent(QEvent *event) override;

private:
    void setupUi();
    void applyStyle(bool hovered);

    CaptureType m_type;
    QLabel *m_iconLabel;
    QLabel *m_titleLabel;
    QLabel *m_descLabel;
    QPushButton *m_screenshotBtn;
    QPushButton *m_recordBtn;
};

#endif // CAPTURECARD_H
