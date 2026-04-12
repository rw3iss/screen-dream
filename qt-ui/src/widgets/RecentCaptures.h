#ifndef RECENTCAPTURES_H
#define RECENTCAPTURES_H

#include <QScrollArea>
#include <QWidget>
#include <QGridLayout>
#include <QLabel>
#include <QString>
#include <QStringList>
#include <QFileInfoList>

class RecentCaptures : public QScrollArea {
    Q_OBJECT

public:
    explicit RecentCaptures(QWidget *parent = nullptr);

    /// Set the directory to scan for captures.
    void setDirectory(const QString &dir);

    /// Rescan the directory and refresh the display.
    void refresh();

signals:
    void fileDoubleClicked(const QString &path);
    void fileOpenRequested(const QString &path);
    void fileCopyPathRequested(const QString &path);
    void fileDeleteRequested(const QString &path);

private:
    void clearThumbnails();
    void showPlaceholder();
    QWidget *createThumbnail(const QFileInfo &fi);

    QString m_directory;
    QWidget *m_container;
    QGridLayout *m_gridLayout;
    QLabel *m_placeholder;
};

#endif // RECENTCAPTURES_H
