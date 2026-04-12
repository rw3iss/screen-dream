#ifndef SOURCEBROWSER_H
#define SOURCEBROWSER_H

#include <QWidget>
#include <QPushButton>
#include <QListWidget>
#include <QLabel>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QThread>
#include <QTimer>
#include <QImage>
#include <QIcon>

#include "core/RustBridge.h"

class SourceBrowser : public QWidget {
    Q_OBJECT

public:
    explicit SourceBrowser(QWidget *parent = nullptr);

    /// Re-query sources from RustBridge asynchronously and refresh the lists.
    void refresh();

private slots:
    void onSourcesLoaded(AvailableSources sources);

    /// Add a saved area to the areas list and persist to disk.
    void addSavedArea(const QString &name, uint32_t monitorId,
                      int32_t x, int32_t y, uint32_t w, uint32_t h);

signals:
    void sourceSelected(CaptureSource source);
    void addAreaRequested();

private slots:
    void toggleExpanded();
    void onMonitorClicked(QListWidgetItem *item);
    void onWindowClicked(QListWidgetItem *item);
    void onAreaClicked(QListWidgetItem *item);
    void onAreaContextMenu(const QPoint &pos);
    void onThumbnailsCaptured(QVector<QImage> thumbnails);

private:
    void setupUi();
    void populateMonitors(const AvailableSources &sources);
    void populateWindows(const AvailableSources &sources);
    void populateAreas();
    void loadSavedAreas();
    void saveSavedAreas();
    QString savedAreasPath() const;
    void startThumbnailTimer();
    void stopThumbnailTimer();
    void requestThumbnailUpdate();

    bool m_expanded = false;
    bool m_loading = false;

    QPushButton *m_toggleBtn;
    QWidget *m_contentWidget;
    QLabel *m_loadingLabel = nullptr;

    // Columns
    QListWidget *m_monitorList;
    QListWidget *m_windowList;
    QListWidget *m_areaList;

    // Cached sources
    AvailableSources m_sources;

    // Saved areas
    QJsonArray m_savedAreas;

    // Thumbnail preview timer
    QTimer *m_thumbnailTimer = nullptr;
    bool m_thumbnailUpdatePending = false;

    // Track selected source type/index
    int m_selectedMonitorRow = -1;
    int m_selectedWindowRow = -1;
    int m_selectedAreaRow = -1;
};

#endif // SOURCEBROWSER_H
