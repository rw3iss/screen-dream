#include "widgets/SourceBrowser.h"
#include "core/AppState.h"

#include <QDir>
#include <QFile>
#include <QJsonDocument>
#include <QInputDialog>
#include <QMenu>
#include <QAction>
#include <QMessageBox>
#include <QStandardPaths>
#include <QFont>
#include <QFrame>
#include <QThread>
#include <QPixmap>
#include <QScrollBar>

// Worker that runs enumerateSources() off the main thread.
class SourceEnumWorker : public QThread {
    Q_OBJECT
public:
    using QThread::QThread;
signals:
    void finished(AvailableSources sources);
protected:
    void run() override {
        AvailableSources result;
        try {
            result = AppState::instance().bridge().enumerateSources();
        } catch (...) {
            // Return empty on failure
        }
        emit finished(result);
    }
};

// Worker that captures monitor thumbnails off the main thread.
// NOTE: Only captures thumbnails for monitors (Screen type), which is fast
// via xcap. Window capture requires portal permission and can be slow, so
// windows show a static icon instead. This limitation can be revisited later.
class ThumbnailCaptureWorker : public QThread {
    Q_OBJECT
public:
    explicit ThumbnailCaptureWorker(QVector<uint32_t> monitorIds, QObject *parent = nullptr)
        : QThread(parent), m_monitorIds(std::move(monitorIds)) {}
signals:
    void finished(QVector<QImage> thumbnails);
protected:
    void run() override {
        QVector<QImage> results;
        results.reserve(m_monitorIds.size());
        for (uint32_t monId : m_monitorIds) {
            CaptureSource src;
            src.type = CaptureSource::Screen;
            src.monitorId = monId;
            try {
                QImage frame = AppState::instance().bridge().captureFrame(src);
                // Scale down to thumbnail size (48x36)
                if (!frame.isNull()) {
                    results.append(frame.scaled(48, 36, Qt::KeepAspectRatio, Qt::SmoothTransformation));
                } else {
                    results.append(QImage());
                }
            } catch (...) {
                results.append(QImage());
            }
        }
        emit finished(results);
    }
private:
    QVector<uint32_t> m_monitorIds;
};

// ---------------------------------------------------------------------------
// SourceBrowser
// ---------------------------------------------------------------------------

SourceBrowser::SourceBrowser(QWidget *parent)
    : QWidget(parent)
{
    setupUi();
    loadSavedAreas();
}

void SourceBrowser::setupUi()
{
    auto *mainLayout = new QVBoxLayout(this);
    mainLayout->setContentsMargins(0, 0, 0, 0);
    mainLayout->setSpacing(0);

    // Toggle button
    m_toggleBtn = new QPushButton(QString::fromUtf8("\u25B6 Browse Sources"), this);
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
    connect(m_toggleBtn, &QPushButton::clicked, this, &SourceBrowser::toggleExpanded);
    mainLayout->addWidget(m_toggleBtn);

    // Content widget (hidden by default) — expands to fill available space
    m_contentWidget = new QWidget(this);
    m_contentWidget->setVisible(false);
    m_contentWidget->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);

    auto *contentLayout = new QHBoxLayout(m_contentWidget);
    contentLayout->setContentsMargins(0, 8, 0, 0);
    contentLayout->setSpacing(20);

    // Common list style
    const QString listStyle =
        "QListWidget {"
        "  background-color: #16213e;"
        "  border: 1px solid #2a2a4a;"
        "  border-radius: 6px;"
        "  color: #e0e0e0;"
        "  font-size: 12px;"
        "  outline: none;"
        "}"
        "QListWidget::item {"
        "  padding: 6px 8px;"
        "  border-bottom: 1px solid #1a1a3a;"
        "}"
        "QListWidget::item:hover {"
        "  background-color: #0f3460;"
        "}"
        "QListWidget::item:selected {"
        "  background-color: #533483;"
        "  color: #ffffff;"
        "}";

    // --- Column 1: Screens ---
    auto *screensCol = new QVBoxLayout();
    auto *screensHeader = new QLabel("Screens", m_contentWidget);
    QFont hdrFont = screensHeader->font();
    hdrFont.setBold(true);
    hdrFont.setPointSize(11);
    screensHeader->setFont(hdrFont);
    screensHeader->setStyleSheet("color: #e0e0e0; padding-bottom: 4px;");
    screensCol->addWidget(screensHeader);

    m_monitorList = new QListWidget(m_contentWidget);
    m_monitorList->setStyleSheet(listStyle);
    m_monitorList->setMinimumHeight(80);
    m_monitorList->setIconSize(QSize(48, 36));
    m_monitorList->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
    connect(m_monitorList, &QListWidget::itemClicked, this, &SourceBrowser::onMonitorClicked);
    screensCol->addWidget(m_monitorList, 1);
    contentLayout->addLayout(screensCol);

    // --- Column 2: Windows ---
    auto *windowsCol = new QVBoxLayout();
    auto *windowsHeader = new QLabel("Windows", m_contentWidget);
    windowsHeader->setFont(hdrFont);
    windowsHeader->setStyleSheet("color: #e0e0e0; padding-bottom: 4px;");
    windowsCol->addWidget(windowsHeader);

    m_windowList = new QListWidget(m_contentWidget);
    m_windowList->setStyleSheet(listStyle);
    m_windowList->setMinimumHeight(80);
    m_windowList->setIconSize(QSize(48, 36));
    m_windowList->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
    connect(m_windowList, &QListWidget::itemClicked, this, &SourceBrowser::onWindowClicked);
    windowsCol->addWidget(m_windowList, 1);
    contentLayout->addLayout(windowsCol);

    // --- Column 3: Saved Areas ---
    auto *areasCol = new QVBoxLayout();
    auto *areasHeader = new QLabel("Saved Areas", m_contentWidget);
    areasHeader->setFont(hdrFont);
    areasHeader->setStyleSheet("color: #e0e0e0; padding-bottom: 4px;");
    areasCol->addWidget(areasHeader);

    m_areaList = new QListWidget(m_contentWidget);
    m_areaList->setStyleSheet(listStyle);
    m_areaList->setMinimumHeight(80);
    m_areaList->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
    m_areaList->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(m_areaList, &QListWidget::itemClicked, this, &SourceBrowser::onAreaClicked);
    connect(m_areaList, &QListWidget::customContextMenuRequested,
            this, &SourceBrowser::onAreaContextMenu);
    areasCol->addWidget(m_areaList);
    contentLayout->addLayout(areasCol);

    m_contentWidget->setLayout(contentLayout);
    mainLayout->addWidget(m_contentWidget, 1);
    // This stretch is always visible and pushes everything above it to the top.
    // When content is expanded, it takes stretch=1 and this stretch=1 shares space.
    // When content is hidden, this stretch takes all remaining space, keeping
    // the toggle button pinned to the top.
    mainLayout->addStretch(1);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
    setLayout(mainLayout);
}

void SourceBrowser::toggleExpanded()
{
    m_expanded = !m_expanded;
    m_contentWidget->setVisible(m_expanded);

    if (m_expanded) {
        m_toggleBtn->setText(QString::fromUtf8("\u25BC Browse Sources"));
        refresh();
    } else {
        m_toggleBtn->setText(QString::fromUtf8("\u25B6 Browse Sources"));
        stopThumbnailTimer();
    }
}

void SourceBrowser::refresh()
{
    if (m_loading)
        return;

    m_loading = true;

    // Show loading state in the lists
    m_monitorList->clear();
    m_windowList->clear();
    auto *loadingItem = new QListWidgetItem("Loading...", m_monitorList);
    loadingItem->setFlags(loadingItem->flags() & ~Qt::ItemIsSelectable);
    loadingItem->setForeground(QColor("#a0a0a0"));
    auto *loadingItem2 = new QListWidgetItem("Loading...", m_windowList);
    loadingItem2->setFlags(loadingItem2->flags() & ~Qt::ItemIsSelectable);
    loadingItem2->setForeground(QColor("#a0a0a0"));

    // Run enumeration on a background thread
    auto *worker = new SourceEnumWorker(this);
    connect(worker, &SourceEnumWorker::finished, this, &SourceBrowser::onSourcesLoaded);
    connect(worker, &SourceEnumWorker::finished, worker, &QObject::deleteLater);
    worker->start();
}

void SourceBrowser::onSourcesLoaded(AvailableSources sources)
{
    m_loading = false;
    m_sources = sources;
    populateMonitors(m_sources);
    populateWindows(m_sources);
    populateAreas();

    // Set a static icon for window items since window capture requires portal
    // permission and can be slow. Monitor thumbnails get live previews below.
    for (int i = 0; i < m_windowList->count(); ++i) {
        QListWidgetItem *item = m_windowList->item(i);
        if (item->flags() & Qt::ItemIsSelectable) {
            // Simple filled rectangle as a window icon placeholder
            QPixmap pix(48, 36);
            pix.fill(QColor("#2a2a4a"));
            item->setIcon(QIcon(pix));
        }
    }

    // Start live thumbnail updates for monitors
    startThumbnailTimer();

    // Trigger an immediate first capture
    requestThumbnailUpdate();
}

// ---------------------------------------------------------------------------
// Populate columns
// ---------------------------------------------------------------------------

void SourceBrowser::populateMonitors(const AvailableSources &sources)
{
    m_monitorList->clear();
    m_selectedMonitorRow = -1;

    for (const auto &mon : sources.monitors) {
        QString text = mon.friendlyName.isEmpty() ? mon.name : mon.friendlyName;
        text += QString("  %1\u00D7%2").arg(mon.width).arg(mon.height);
        if (mon.isPrimary)
            text += "  [primary]";

        auto *item = new QListWidgetItem(text, m_monitorList);
        item->setData(Qt::UserRole, mon.id);
    }
}

void SourceBrowser::populateWindows(const AvailableSources &sources)
{
    m_windowList->clear();
    m_selectedWindowRow = -1;

    if (sources.windowsUnavailable) {
        auto *item = new QListWidgetItem(
            sources.windowsUnavailableReason.isEmpty()
                ? "Window listing unavailable on this platform"
                : sources.windowsUnavailableReason,
            m_windowList);
        item->setFlags(item->flags() & ~Qt::ItemIsSelectable);
        item->setForeground(QColor("#808080"));
        return;
    }

    for (const auto &win : sources.windows) {
        // Truncate long titles
        QString title = win.title;
        if (title.length() > 40)
            title = title.left(37) + "...";

        auto *item = new QListWidgetItem(m_windowList);
        item->setText(title);
        item->setToolTip(win.title);
        item->setData(Qt::UserRole, win.id);

        // Show app name in muted text as part of tooltip; we'll use
        // a two-line display via item size hint
        if (!win.appName.isEmpty()) {
            item->setText(title + "\n" + win.appName);
            QFont f = m_windowList->font();
            f.setPointSize(10);
            // The second line (app name) appears naturally via newline
        }
    }
}

void SourceBrowser::populateAreas()
{
    m_areaList->clear();
    m_selectedAreaRow = -1;

    for (int i = 0; i < m_savedAreas.size(); ++i) {
        QJsonObject area = m_savedAreas[i].toObject();
        QString name = area.value("name").toString(QString("Area %1").arg(i + 1));
        int w = area.value("width").toInt();
        int h = area.value("height").toInt();
        int x = area.value("x").toInt();
        int y = area.value("y").toInt();

        QString text = QString("%1\n%2\u00D7%3 @ %4,%5").arg(name).arg(w).arg(h).arg(x).arg(y);
        auto *item = new QListWidgetItem(text, m_areaList);
        item->setData(Qt::UserRole, i);
    }

    // Add "+" card
    auto *addItem = new QListWidgetItem("+ Save New Area", m_areaList);
    addItem->setData(Qt::UserRole, -1);
    addItem->setForeground(QColor("#533483"));
    QFont f = m_areaList->font();
    f.setBold(true);
    addItem->setFont(f);
}

// ---------------------------------------------------------------------------
// Click handlers
// ---------------------------------------------------------------------------

void SourceBrowser::onMonitorClicked(QListWidgetItem *item)
{
    // Deselect other lists
    m_windowList->clearSelection();
    m_areaList->clearSelection();

    uint32_t monId = item->data(Qt::UserRole).toUInt();
    m_selectedMonitorRow = m_monitorList->row(item);

    CaptureSource src;
    src.type = CaptureSource::Screen;
    src.monitorId = monId;
    emit sourceSelected(src);
}

void SourceBrowser::onWindowClicked(QListWidgetItem *item)
{
    if (!(item->flags() & Qt::ItemIsSelectable))
        return;

    m_monitorList->clearSelection();
    m_areaList->clearSelection();

    uint32_t winId = item->data(Qt::UserRole).toUInt();
    m_selectedWindowRow = m_windowList->row(item);

    CaptureSource src;
    src.type = CaptureSource::Window;
    src.windowId = winId;
    emit sourceSelected(src);
}

void SourceBrowser::onAreaClicked(QListWidgetItem *item)
{
    int idx = item->data(Qt::UserRole).toInt();
    if (idx < 0) {
        // "+" card clicked
        emit addAreaRequested();
        return;
    }

    m_monitorList->clearSelection();
    m_windowList->clearSelection();
    m_selectedAreaRow = m_areaList->row(item);

    QJsonObject area = m_savedAreas[idx].toObject();
    CaptureSource src;
    src.type = CaptureSource::Region;
    src.monitorId = area.value("monitor_id").toInt(0);
    src.regionX = area.value("x").toInt();
    src.regionY = area.value("y").toInt();
    src.regionWidth = area.value("width").toInt();
    src.regionHeight = area.value("height").toInt();
    emit sourceSelected(src);
}

// ---------------------------------------------------------------------------
// Context menu for saved areas
// ---------------------------------------------------------------------------

void SourceBrowser::onAreaContextMenu(const QPoint &pos)
{
    QListWidgetItem *item = m_areaList->itemAt(pos);
    if (!item) return;

    int idx = item->data(Qt::UserRole).toInt();
    if (idx < 0) return;  // "+" card

    QMenu menu(this);
    QAction *renameAct = menu.addAction("Rename");
    QAction *deleteAct = menu.addAction("Delete");

    QAction *chosen = menu.exec(m_areaList->mapToGlobal(pos));
    if (!chosen) return;

    if (chosen == renameAct) {
        QJsonObject area = m_savedAreas[idx].toObject();
        bool ok = false;
        QString newName = QInputDialog::getText(
            this, "Rename Area", "Name:",
            QLineEdit::Normal, area.value("name").toString(), &ok);
        if (ok && !newName.isEmpty()) {
            area["name"] = newName;
            m_savedAreas[idx] = area;
            saveSavedAreas();
            populateAreas();
        }
    } else if (chosen == deleteAct) {
        auto reply = QMessageBox::question(this, "Delete Area",
            "Delete this saved area?",
            QMessageBox::Yes | QMessageBox::No);
        if (reply == QMessageBox::Yes) {
            m_savedAreas.removeAt(idx);
            saveSavedAreas();
            populateAreas();
        }
    }
}

// ---------------------------------------------------------------------------
// Saved areas persistence
// ---------------------------------------------------------------------------

QString SourceBrowser::savedAreasPath() const
{
    QString configDir = QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation);
    return configDir + "/com.screendream.app/saved_areas.json";
}

void SourceBrowser::loadSavedAreas()
{
    QFile file(savedAreasPath());
    if (!file.exists()) {
        m_savedAreas = QJsonArray();
        return;
    }
    if (!file.open(QIODevice::ReadOnly)) {
        m_savedAreas = QJsonArray();
        return;
    }
    QJsonDocument doc = QJsonDocument::fromJson(file.readAll());
    file.close();
    if (doc.isArray())
        m_savedAreas = doc.array();
    else
        m_savedAreas = QJsonArray();
}

void SourceBrowser::saveSavedAreas()
{
    QString path = savedAreasPath();
    QDir().mkpath(QFileInfo(path).absolutePath());

    QFile file(path);
    if (!file.open(QIODevice::WriteOnly)) return;
    file.write(QJsonDocument(m_savedAreas).toJson());
    file.close();
}

void SourceBrowser::addSavedArea(const QString &name, uint32_t monitorId,
                                  int32_t x, int32_t y, uint32_t w, uint32_t h)
{
    QJsonObject area;
    area["name"] = name;
    area["monitor_id"] = static_cast<int>(monitorId);
    area["x"] = x;
    area["y"] = y;
    area["width"] = static_cast<int>(w);
    area["height"] = static_cast<int>(h);

    m_savedAreas.append(area);
    saveSavedAreas();

    if (m_expanded)
        populateAreas();
}

// ---------------------------------------------------------------------------
// Thumbnail preview updates
// ---------------------------------------------------------------------------

void SourceBrowser::startThumbnailTimer()
{
    if (!m_thumbnailTimer) {
        m_thumbnailTimer = new QTimer(this);
        m_thumbnailTimer->setInterval(500);
        connect(m_thumbnailTimer, &QTimer::timeout, this, &SourceBrowser::requestThumbnailUpdate);
    }
    m_thumbnailTimer->start();
}

void SourceBrowser::stopThumbnailTimer()
{
    if (m_thumbnailTimer)
        m_thumbnailTimer->stop();
}

void SourceBrowser::requestThumbnailUpdate()
{
    // Don't queue another update if one is already in flight
    if (m_thumbnailUpdatePending)
        return;

    // Collect monitor IDs for visible items only
    QVector<uint32_t> visibleMonitorIds;
    for (int i = 0; i < m_monitorList->count(); ++i) {
        QListWidgetItem *item = m_monitorList->item(i);
        // Check if item is visible in the viewport
        QRect itemRect = m_monitorList->visualItemRect(item);
        if (m_monitorList->viewport()->rect().intersects(itemRect)) {
            visibleMonitorIds.append(item->data(Qt::UserRole).toUInt());
        }
    }

    if (visibleMonitorIds.isEmpty())
        return;

    m_thumbnailUpdatePending = true;

    auto *worker = new ThumbnailCaptureWorker(visibleMonitorIds, this);
    connect(worker, &ThumbnailCaptureWorker::finished, this, &SourceBrowser::onThumbnailsCaptured);
    connect(worker, &ThumbnailCaptureWorker::finished, worker, &QObject::deleteLater);
    worker->start();
}

void SourceBrowser::onThumbnailsCaptured(QVector<QImage> thumbnails)
{
    m_thumbnailUpdatePending = false;

    // Map thumbnails back to visible monitor list items
    int thumbIdx = 0;
    for (int i = 0; i < m_monitorList->count() && thumbIdx < thumbnails.size(); ++i) {
        QListWidgetItem *item = m_monitorList->item(i);
        QRect itemRect = m_monitorList->visualItemRect(item);
        if (m_monitorList->viewport()->rect().intersects(itemRect)) {
            const QImage &thumb = thumbnails[thumbIdx++];
            if (!thumb.isNull()) {
                item->setIcon(QIcon(QPixmap::fromImage(thumb)));
            }
        }
    }
}

// Required for Q_OBJECT in .cpp file
#include "SourceBrowser.moc"
