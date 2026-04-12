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
#include <algorithm>

// ---------------------------------------------------------------------------
// Clickable thumbnail widget (internal)
// ---------------------------------------------------------------------------

class ThumbnailWidget : public QWidget {
public:
    explicit ThumbnailWidget(const QString &filePath, QWidget *parent = nullptr)
        : QWidget(parent), m_filePath(filePath) {}

    QString filePath() const { return m_filePath; }

protected:
    void mouseDoubleClickEvent(QMouseEvent *event) override {
        Q_UNUSED(event);
        // Bubble up — RecentCaptures connects via event filter
        if (auto *rc = qobject_cast<RecentCaptures *>(parent()->parent()->parent())) {
            emit rc->fileDoubleClicked(m_filePath);
        }
    }

    void contextMenuEvent(QContextMenuEvent *event) override {
        QMenu menu(this);
        QAction *openAct = menu.addAction("Open");
        QAction *copyAct = menu.addAction("Copy Path");
        menu.addSeparator();
        QAction *deleteAct = menu.addAction("Delete");

        QAction *chosen = menu.exec(event->globalPos());
        if (!chosen) return;

        if (auto *rc = qobject_cast<RecentCaptures *>(parent()->parent()->parent())) {
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
    : QScrollArea(parent), m_placeholder(nullptr)
{
    setWidgetResizable(true);
    setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    setFrameShape(QFrame::NoFrame);

    m_container = new QWidget(this);
    m_gridLayout = new QGridLayout(m_container);
    m_gridLayout->setSpacing(10);
    m_gridLayout->setContentsMargins(8, 8, 8, 8);
    m_gridLayout->setAlignment(Qt::AlignTop | Qt::AlignLeft);
    m_container->setLayout(m_gridLayout);
    setWidget(m_container);

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

    showPlaceholder();
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
        showPlaceholder();
        return;
    }

    QDir dir(m_directory);
    QStringList filters;
    filters << "*.png" << "*.jpg" << "*.jpeg" << "*.mp4" << "*.webm";
    dir.setNameFilters(filters);
    dir.setSorting(QDir::Time);

    QFileInfoList files = dir.entryInfoList(QDir::Files, QDir::Time);
    if (files.isEmpty()) {
        showPlaceholder();
        return;
    }

    // Show up to 30 recent files
    int maxFiles = qMin(files.size(), static_cast<qsizetype>(30));
    int cols = 6;
    for (int i = 0; i < maxFiles; ++i) {
        QWidget *thumb = createThumbnail(files[i]);
        m_gridLayout->addWidget(thumb, i / cols, i % cols);
    }
}

void RecentCaptures::clearThumbnails()
{
    QLayoutItem *child;
    while ((child = m_gridLayout->takeAt(0)) != nullptr) {
        if (child->widget())
            child->widget()->deleteLater();
        delete child;
    }
    m_placeholder = nullptr;
}

void RecentCaptures::showPlaceholder()
{
    m_placeholder = new QLabel("No captures yet", m_container);
    m_placeholder->setAlignment(Qt::AlignCenter);
    m_placeholder->setStyleSheet("color: #606060; font-size: 14px; padding: 20px;");
    m_gridLayout->addWidget(m_placeholder, 0, 0, 1, 6, Qt::AlignCenter);
}

QWidget *RecentCaptures::createThumbnail(const QFileInfo &fi)
{
    auto *widget = new ThumbnailWidget(fi.absoluteFilePath(), m_container);
    widget->setFixedSize(100, 100);
    widget->setCursor(Qt::PointingHandCursor);
    widget->setToolTip(fi.fileName());
    widget->setStyleSheet(
        "ThumbnailWidget {"
        "  background-color: #16213e;"
        "  border: 1px solid #2a2a4a;"
        "  border-radius: 6px;"
        "}"
        "ThumbnailWidget:hover {"
        "  border-color: #533483;"
        "}"
    );

    auto *layout = new QVBoxLayout(widget);
    layout->setContentsMargins(4, 4, 4, 4);
    layout->setSpacing(2);
    layout->setAlignment(Qt::AlignCenter);

    // Thumbnail image or icon
    auto *imgLabel = new QLabel(widget);
    imgLabel->setAlignment(Qt::AlignCenter);
    imgLabel->setFixedSize(80, 60);
    imgLabel->setStyleSheet("background-color: transparent; border: none;");

    QString suffix = fi.suffix().toLower();
    if (suffix == "mp4" || suffix == "webm") {
        imgLabel->setText(QString::fromUtf8("\xF0\x9F\x8E\xAC"));  // movie emoji
        QFont f = imgLabel->font();
        f.setPointSize(20);
        imgLabel->setFont(f);
    } else {
        QPixmap pix(fi.absoluteFilePath());
        if (!pix.isNull()) {
            imgLabel->setPixmap(pix.scaled(80, 60, Qt::KeepAspectRatio, Qt::SmoothTransformation));
        } else {
            imgLabel->setText(QString::fromUtf8("\xF0\x9F\x96\xBC"));  // picture emoji
            QFont f = imgLabel->font();
            f.setPointSize(20);
            imgLabel->setFont(f);
        }
    }

    auto *nameLabel = new QLabel(fi.fileName(), widget);
    nameLabel->setAlignment(Qt::AlignCenter);
    nameLabel->setStyleSheet("color: #a0a0a0; font-size: 10px; background-color: transparent; border: none;");
    nameLabel->setMaximumWidth(90);
    nameLabel->setWordWrap(false);

    // Elide text if too long
    QFontMetrics fm(nameLabel->font());
    nameLabel->setText(fm.elidedText(fi.fileName(), Qt::ElideMiddle, 88));

    layout->addWidget(imgLabel);
    layout->addWidget(nameLabel);

    return widget;
}
