#ifndef RECENTCAPTURES_H
#define RECENTCAPTURES_H

#include <QWidget>
#include <QScrollArea>
#include <QHBoxLayout>
#include <QVBoxLayout>
#include <QPushButton>
#include <QLabel>
#include <QString>
#include <QStringList>
#include <QFileInfoList>

class RecentCaptures : public QWidget {
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

private slots:
    void toggleExpanded();

private:
    void clearThumbnails();
    void showPlaceholder();
    QWidget *createThumbnail(const QFileInfo &fi);
    void updateHeaderText();

    bool m_expanded = false;
    QString m_directory;
    int m_fileCount = 0;

    QPushButton *m_toggleBtn;
    QWidget *m_contentWidget;
    QScrollArea *m_scrollArea;
    QHBoxLayout *m_itemsLayout;
    QLabel *m_placeholder;
};

#endif // RECENTCAPTURES_H
