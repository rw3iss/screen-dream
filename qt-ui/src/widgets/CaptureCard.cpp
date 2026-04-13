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
    setFixedHeight(60);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);

    auto *layout = new QHBoxLayout(this);
    layout->setContentsMargins(10, 6, 10, 6);
    layout->setSpacing(8);

    // Left side: icon + title + description
    auto *infoLayout = new QVBoxLayout();
    infoLayout->setContentsMargins(0, 0, 0, 0);
    infoLayout->setSpacing(1);

    // Title row with small inline icon
    auto *titleRow = new QHBoxLayout();
    titleRow->setContentsMargins(0, 0, 0, 0);
    titleRow->setSpacing(4);

    m_iconLabel = new QLabel(this);
    QFont iconFont = m_iconLabel->font();
    iconFont.setPointSize(11);
    m_iconLabel->setFont(iconFont);

    m_titleLabel = new QLabel(this);
    QFont titleFont = m_titleLabel->font();
    titleFont.setPointSize(11);
    titleFont.setBold(true);
    m_titleLabel->setFont(titleFont);
    m_titleLabel->setStyleSheet("color: #e0e0e0;");

    titleRow->addWidget(m_iconLabel);
    titleRow->addWidget(m_titleLabel);
    titleRow->addStretch();

    // Description
    m_descLabel = new QLabel(this);
    m_descLabel->setStyleSheet("color: #707090; font-size: 10px;");

    switch (m_type) {
    case Screen:
        m_iconLabel->setText(QString::fromUtf8("\xF0\x9F\x96\xA5"));
        m_titleLabel->setText("SCREEN");
        m_descLabel->setText("Full screen capture");
        break;
    case Window:
        m_iconLabel->setText(QString::fromUtf8("\xF0\x9F\xAA\x9F"));
        m_titleLabel->setText("WINDOW");
        m_descLabel->setText("Single window capture");
        break;
    case Area:
        m_iconLabel->setText(QString::fromUtf8("\xE2\x9C\x82"));
        m_titleLabel->setText("AREA");
        m_descLabel->setText("Select region to capture");
        break;
    }

    infoLayout->addLayout(titleRow);
    infoLayout->addWidget(m_descLabel);

    layout->addLayout(infoLayout, 1);

    // Right side: buttons
    auto *btnLayout = new QHBoxLayout();
    btnLayout->setSpacing(4);

    m_screenshotBtn = new QPushButton("Screenshot", this);
    m_screenshotBtn->setToolTip("Take a screenshot");
    m_screenshotBtn->setCursor(Qt::PointingHandCursor);
    m_screenshotBtn->setFixedHeight(26);
    m_screenshotBtn->setStyleSheet(
        "QPushButton { background-color: #1a1a3a; color: #c0c0c0; border: 1px solid #2a2a4a; border-radius: 4px; padding: 2px 8px; font-size: 11px; }"
        "QPushButton:hover { background-color: #2a2a5a; color: #ffffff; border-color: #3a3a6a; }"
        "QPushButton:pressed { background-color: #0f0f2f; }"
    );

    m_recordBtn = new QPushButton("Record", this);
    m_recordBtn->setToolTip("Start recording");
    m_recordBtn->setCursor(Qt::PointingHandCursor);
    m_recordBtn->setFixedHeight(26);
    m_recordBtn->setStyleSheet(
        "QPushButton { background-color: #e94560; color: #ffffff; border: 1px solid #e94560; border-radius: 4px; padding: 2px 8px; font-size: 11px; }"
        "QPushButton:hover { background-color: #ff5a7a; border-color: #ff5a7a; }"
        "QPushButton:pressed { background-color: #c73550; }"
    );

    btnLayout->addWidget(m_screenshotBtn);
    btnLayout->addWidget(m_recordBtn);
    layout->addLayout(btnLayout, 0);

    connect(m_screenshotBtn, &QPushButton::clicked, this, &CaptureCard::screenshotClicked);
    connect(m_recordBtn, &QPushButton::clicked, this, &CaptureCard::recordClicked);
}

void CaptureCard::setRecordingState(bool recording)
{
    if (recording) {
        m_recordBtn->setText("Stop");
        m_recordBtn->setToolTip("Stop recording");
        m_recordBtn->setStyleSheet(
            "QPushButton { background-color: #cc0000; color: #ffffff; border: 1px solid #cc0000; border-radius: 4px; padding: 2px 8px; font-size: 11px; font-weight: bold; }"
            "QPushButton:hover { background-color: #ee2222; border-color: #ee2222; }"
            "QPushButton:pressed { background-color: #aa0000; }"
        );
    } else {
        m_recordBtn->setText("Record");
        m_recordBtn->setToolTip("Start recording");
        m_recordBtn->setStyleSheet(
            "QPushButton { background-color: #e94560; color: #ffffff; border: 1px solid #e94560; border-radius: 4px; padding: 2px 8px; font-size: 11px; }"
            "QPushButton:hover { background-color: #ff5a7a; border-color: #ff5a7a; }"
            "QPushButton:pressed { background-color: #c73550; }"
        );
    }
}

void CaptureCard::applyStyle(bool hovered)
{
    QString bg = hovered ? "#0f3460" : "#16213e";
    setStyleSheet(QString(
        "CaptureCard {"
        "  background-color: %1;"
        "  border: 1px solid #2a2a4a;"
        "  border-radius: 6px;"
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
