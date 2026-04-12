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

#include "core/RustBridge.h"

class SourceBrowser : public QWidget {
    Q_OBJECT

public:
    explicit SourceBrowser(QWidget *parent = nullptr);

    /// Re-query sources from RustBridge and refresh the lists.
    void refresh();

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

private:
    void setupUi();
    void populateMonitors(const AvailableSources &sources);
    void populateWindows(const AvailableSources &sources);
    void populateAreas();
    void loadSavedAreas();
    void saveSavedAreas();
    QString savedAreasPath() const;

    bool m_expanded = false;

    QPushButton *m_toggleBtn;
    QWidget *m_contentWidget;

    // Columns
    QListWidget *m_monitorList;
    QListWidget *m_windowList;
    QListWidget *m_areaList;

    // Cached sources
    AvailableSources m_sources;

    // Saved areas
    QJsonArray m_savedAreas;

    // Track selected source type/index
    int m_selectedMonitorRow = -1;
    int m_selectedWindowRow = -1;
    int m_selectedAreaRow = -1;
};

#endif // SOURCEBROWSER_H
