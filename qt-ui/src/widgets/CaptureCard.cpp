#include "widgets/CaptureCard.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QFont>
#include <QEnterEvent>

CaptureCard::CaptureCard(CaptureType type, QWidget *parent)
    : QWidget(parent), m_type(type)
{
    setupUi();
    applyStyle(false);
}

void CaptureCard::setupUi()
{
    setFixedSize(200, 180);

    auto *layout = new QVBoxLayout(this);
    layout->setAlignment(Qt::AlignCenter);
    layout->setSpacing(6);
    layout->setContentsMargins(12, 12, 12, 12);

    // Icon
    m_iconLabel = new QLabel(this);
    m_iconLabel->setAlignment(Qt::AlignCenter);
    QFont iconFont = m_iconLabel->font();
    iconFont.setPointSize(28);
    m_iconLabel->setFont(iconFont);

    // Title
    m_titleLabel = new QLabel(this);
    m_titleLabel->setAlignment(Qt::AlignCenter);
    QFont titleFont = m_titleLabel->font();
    titleFont.setPointSize(12);
    titleFont.setBold(true);
    m_titleLabel->setFont(titleFont);

    // Description
    m_descLabel = new QLabel(this);
    m_descLabel->setAlignment(Qt::AlignCenter);
    m_descLabel->setStyleSheet("color: #a0a0a0; font-size: 12px;");

    switch (m_type) {
    case Screen:
        m_iconLabel->setText(QString::fromUtf8("\xF0\x9F\x96\xA5"));  // desktop monitor emoji
        m_titleLabel->setText("SCREEN");
        m_descLabel->setText("Full screen capture");
        break;
    case Window:
        m_iconLabel->setText(QString::fromUtf8("\xF0\x9F\xAA\x9F"));  // window emoji
        m_titleLabel->setText("WINDOW");
        m_descLabel->setText("Single window capture");
        break;
    case Area:
        m_iconLabel->setText(QString::fromUtf8("\xE2\x9C\x82"));  // scissors emoji
        m_titleLabel->setText("AREA");
        m_descLabel->setText("Select region to capture");
        break;
    }

    layout->addWidget(m_iconLabel);
    layout->addWidget(m_titleLabel);
    layout->addWidget(m_descLabel);

    // Buttons row
    auto *btnLayout = new QHBoxLayout();
    btnLayout->setSpacing(8);

    m_screenshotBtn = new QPushButton("Screenshot", this);
    m_screenshotBtn->setToolTip("Take a screenshot");
    m_screenshotBtn->setCursor(Qt::PointingHandCursor);

    m_recordBtn = new QPushButton("Record", this);
    m_recordBtn->setToolTip("Start recording");
    m_recordBtn->setCursor(Qt::PointingHandCursor);
    m_recordBtn->setStyleSheet(
        "QPushButton { background-color: #e94560; color: #ffffff; border: 1px solid #e94560; border-radius: 6px; padding: 4px 10px; }"
        "QPushButton:hover { background-color: #ff5a7a; border-color: #ff5a7a; }"
        "QPushButton:pressed { background-color: #c73550; }"
    );

    btnLayout->addWidget(m_screenshotBtn);
    btnLayout->addWidget(m_recordBtn);
    layout->addLayout(btnLayout);

    connect(m_screenshotBtn, &QPushButton::clicked, this, &CaptureCard::screenshotClicked);
    connect(m_recordBtn, &QPushButton::clicked, this, &CaptureCard::recordClicked);
}

void CaptureCard::applyStyle(bool hovered)
{
    QString bg = hovered ? "#0f3460" : "#16213e";
    setStyleSheet(QString(
        "CaptureCard {"
        "  background-color: %1;"
        "  border: 1px solid #2a2a4a;"
        "  border-radius: 10px;"
        "}"
    ).arg(bg));
}

void CaptureCard::enterEvent(QEnterEvent *event)
{
    applyStyle(true);
    QWidget::enterEvent(event);
}

void CaptureCard::leaveEvent(QEvent *event)
{
    applyStyle(false);
    QWidget::leaveEvent(event);
}
