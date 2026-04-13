#include "widgets/RecentCaptures.h"

#include <QDir>
#include <QFileInfo>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QPixmap>
#include <QMenu>
#include <QAction>
#include <QMouseEvent>
#include <QDesktopServices>
#include <QUrl>
#include <QApplication>
#include <QClipboard>
#include <QFile>
#include <QMessageBox>
#include <QDateTime>
#include <QScrollBar>
#include <algorithm>

// ---------------------------------------------------------------------------
// Clickable thumbnail widget (internal)
// ---------------------------------------------------------------------------

class ThumbnailWidget : public QWidget {
public:
    explicit ThumbnailWidget(const QString &filePath, QWidget *parent = nullptr)
        : QWidget(parent), m_filePath(filePath)
    {
        setCursor(Qt::PointingHandCursor);
    }

    QString filePath() const { return m_filePath; }

protected:
    void mousePressEvent(QMouseEvent *event) override {
        if (event->button() == Qt::LeftButton) {
            QDesktopServices::openUrl(QUrl::fromLocalFile(m_filePath));
        }
        QWidget::mousePressEvent(event);
    }

    void contextMenuEvent(QContextMenuEvent *event) override {
        QMenu menu(this);
        QAction *openAct = menu.addAction("Open");
        QAction *copyAct = menu.addAction("Copy Path");
        menu.addSeparator();
        QAction *deleteAct = menu.addAction("Delete");

        QAction *chosen = menu.exec(event->globalPos());
        if (!chosen) return;

        // Walk up to find RecentCaptures parent
        QWidget *p = parentWidget();
        while (p && !qobject_cast<RecentCaptures *>(p))
            p = p->parentWidget();
        if (auto *rc = qobject_cast<RecentCaptures *>(p)) {
            if (chosen == openAct)
                emit rc->fileOpenRequested(m_filePath);
            else if (chosen == copyAct)
                emit rc->fileCopyPathRequested(m_filePath);
            else if (chosen == deleteAct)
                emit rc->fileDeleteRequested(m_filePath);
        }
    }

private:
    QString m_filePath;
};

// ---------------------------------------------------------------------------
// RecentCaptures
// ---------------------------------------------------------------------------

RecentCaptures::RecentCaptures(QWidget *parent)
    : QWidget(parent), m_placeholder(nullptr)
{
    auto *mainLayout = new QVBoxLayout(this);
    mainLayout->setContentsMargins(0, 0, 0, 0);
    mainLayout->setSpacing(0);

    // Toggle button header
    m_toggleBtn = new QPushButton(QString::fromUtf8("\u25B6 Recent Captures (0)"), this);
    m_toggleBtn->setFlat(true);
    m_toggleBtn->setCursor(Qt::PointingHandCursor);
    m_toggleBtn->setStyleSheet(
        "QPushButton {"
        "  color: #a0a0a0; font-size: 13px; font-weight: bold;"
        "  border: none; padding: 6px 0; background: transparent;"
        "  text-align: left;"
        "}"
        "QPushButton:hover { color: #e0e0e0; }"
    );
    connect(m_toggleBtn, &QPushButton::clicked, this, &RecentCaptures::toggleExpanded);
    mainLayout->addWidget(m_toggleBtn);

    // Content widget (hidden by default)
    m_contentWidget = new QWidget(this);
    m_contentWidget->setVisible(false);
    m_contentWidget->setMaximumHeight(140);

    auto *contentLayout = new QVBoxLayout(m_contentWidget);
    contentLayout->setContentsMargins(0, 4, 0, 0);
    contentLayout->setSpacing(0);

    // Horizontal scroll area
    m_scrollArea = new QScrollArea(m_contentWidget);
    m_scrollArea->setWidgetResizable(true);
    m_scrollArea->setHorizontalScrollBarPolicy(Qt::ScrollBarAsNeeded);
    m_scrollArea->setVerticalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_scrollArea->setFrameShape(QFrame::NoFrame);
    m_scrollArea->setFixedHeight(120);
    m_scrollArea->setStyleSheet(
        "QScrollArea { background: transparent; }"
        "QScrollBar:horizontal {"
        "  height: 6px; background: transparent;"
        "}"
        "QScrollBar::handle:horizontal {"
        "  background: #2a2a4a; border-radius: 3px; min-width: 30px;"
        "}"
        "QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal { width: 0; }"
    );

    auto *scrollContent = new QWidget(m_scrollArea);
    m_itemsLayout = new QHBoxLayout(scrollContent);
    m_itemsLayout->setContentsMargins(4, 4, 4, 4);
    m_itemsLayout->setSpacing(8);
    m_itemsLayout->setAlignment(Qt::AlignLeft | Qt::AlignVCenter);
    scrollContent->setLayout(m_itemsLayout);
    m_scrollArea->setWidget(scrollContent);

    contentLayout->addWidget(m_scrollArea);
    m_contentWidget->setLayout(contentLayout);
    mainLayout->addWidget(m_contentWidget);

    setLayout(mainLayout);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);

    // Default signal handlers
    connect(this, &RecentCaptures::fileOpenRequested, this, [](const QString &path) {
        QDesktopServices::openUrl(QUrl::fromLocalFile(path));
    });
    connect(this, &RecentCaptures::fileCopyPathRequested, this, [](const QString &path) {
        QApplication::clipboard()->setText(path);
    });
    connect(this, &RecentCaptures::fileDeleteRequested, this, [this](const QString &path) {
        auto reply = QMessageBox::question(this, "Delete File",
            QString("Delete %1?").arg(QFileInfo(path).fileName()),
            QMessageBox::Yes | QMessageBox::No);
        if (reply == QMessageBox::Yes) {
            QFile::remove(path);
            refresh();
        }
    });
    connect(this, &RecentCaptures::fileDoubleClicked, this, [](const QString &path) {
        QDesktopServices::openUrl(QUrl::fromLocalFile(path));
    });
}

void RecentCaptures::toggleExpanded()
{
    m_expanded = !m_expanded;
    m_contentWidget->setVisible(m_expanded);
    updateHeaderText();
}

void RecentCaptures::updateHeaderText()
{
    QString arrow = m_expanded ? QString::fromUtf8("\u25BC") : QString::fromUtf8("\u25B6");
    m_toggleBtn->setText(QString("%1 Recent Captures (%2)").arg(arrow).arg(m_fileCount));
}

void RecentCaptures::setDirectory(const QString &dir)
{
    m_directory = dir;
    refresh();
}

void RecentCaptures::refresh()
{
    clearThumbnails();

    if (m_directory.isEmpty()) {
        m_fileCount = 0;
        updateHeaderText();
        showPlaceholder();
        return;
    }

    QDir dir(m_directory);
    QStringList filters;
    filters << "*.png" << "*.jpg" << "*.jpeg" << "*.mp4" << "*.webm";
    dir.setNameFilters(filters);
    dir.setSorting(QDir::Time);

    QFileInfoList files = dir.entryInfoList(QDir::Files, QDir::Time);
    m_fileCount = files.size();
    updateHeaderText();

    if (files.isEmpty()) {
        showPlaceholder();
        return;
    }

    // Show up to 30 recent files in a horizontal row
    int maxFiles = qMin(files.size(), static_cast<qsizetype>(30));
    for (int i = 0; i < maxFiles; ++i) {
        QWidget *thumb = createThumbnail(files[i]);
        m_itemsLayout->addWidget(thumb);
    }
    m_itemsLayout->addStretch();
}

void RecentCaptures::clearThumbnails()
{
    QLayoutItem *child;
    while ((child = m_itemsLayout->takeAt(0)) != nullptr) {
        if (child->widget())
            child->widget()->deleteLater();
        delete child;
    }
    m_placeholder = nullptr;
}

void RecentCaptures::showPlaceholder()
{
    m_placeholder = new QLabel("No captures yet", m_scrollArea->widget());
    m_placeholder->setAlignment(Qt::AlignCenter);
    m_placeholder->setStyleSheet("color: #606060; font-size: 12px; padding: 10px;");
    m_itemsLayout->addWidget(m_placeholder);
}

QWidget *RecentCaptures::createThumbnail(const QFileInfo &fi)
{
    auto *widget = new ThumbnailWidget(fi.absoluteFilePath(), m_scrollArea->widget());
    widget->setFixedSize(90, 100);
    widget->setToolTip(fi.fileName());
    widget->setStyleSheet(
        "ThumbnailWidget {"
        "  background-color: #16213e;"
        "  border: 1px solid #2a2a4a;"
        "  border-radius: 4px;"
        "}"
        "ThumbnailWidget:hover {"
        "  border-color: #533483;"
        "}"
    );

    auto *layout = new QVBoxLayout(widget);
    layout->setContentsMargins(3, 3, 3, 3);
    layout->setSpacing(1);
    layout->setAlignment(Qt::AlignCenter);

    // Thumbnail image or icon
    auto *imgLabel = new QLabel(widget);
    imgLabel->setAlignment(Qt::AlignCenter);
    imgLabel->setFixedSize(80, 55);
    imgLabel->setStyleSheet("background-color: transparent; border: none;");

    QString suffix = fi.suffix().toLower();
    if (suffix == "mp4" || suffix == "webm") {
        imgLabel->setText(QString::fromUtf8("\xF0\x9F\x8E\xAC"));
        QFont f = imgLabel->font();
        f.setPointSize(16);
        imgLabel->setFont(f);
    } else {
        QPixmap pix(fi.absoluteFilePath());
        if (!pix.isNull()) {
            imgLabel->setPixmap(pix.scaled(80, 55, Qt::KeepAspectRatio, Qt::SmoothTransformation));
        } else {
            imgLabel->setText(QString::fromUtf8("\xF0\x9F\x96\xBC"));
            QFont f = imgLabel->font();
            f.setPointSize(16);
            imgLabel->setFont(f);
        }
    }

    // Filename
    auto *nameLabel = new QLabel(widget);
    nameLabel->setAlignment(Qt::AlignCenter);
    nameLabel->setStyleSheet("color: #a0a0a0; font-size: 9px; background-color: transparent; border: none;");
    nameLabel->setMaximumWidth(82);
    QFontMetrics fm(nameLabel->font());
    nameLabel->setText(fm.elidedText(fi.fileName(), Qt::ElideMiddle, 80));

    // Type label
    QString typeText;
    QString fileNameLower = fi.fileName().toLower();
    if (fileNameLower.contains("screenshot")) {
        typeText = "IMAGE";
    } else if (fileNameLower.contains("recording")) {
        typeText = "VIDEO";
    } else if (suffix == "mp4" || suffix == "webm" || suffix == "mkv") {
        typeText = "VIDEO";
    } else if (suffix == "png" || suffix == "jpg" || suffix == "jpeg" || suffix == "webp") {
        typeText = "IMAGE";
    }

    auto *typeLabel = new QLabel(typeText, widget);
    typeLabel->setAlignment(Qt::AlignCenter);
    typeLabel->setStyleSheet(
        "color: #707090; font-size: 8px; font-weight: bold; "
        "background-color: transparent; border: none;"
    );

    layout->addWidget(imgLabel);
    layout->addWidget(nameLabel);
    layout->addWidget(typeLabel);

    return widget;
}
