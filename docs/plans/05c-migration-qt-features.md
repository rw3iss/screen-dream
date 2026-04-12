# Plan 5c: Qt6+Rust Migration — Recording Flow, Editor, Settings & Packaging

> **License:** GPLv3

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**App Name:** Screen Dream (configured via a single `APP_CONFIG` constant — all code references import from `app_config`)

**Goal:** Implement the complete recording UI flow (source picker, region selector, recorder panel), system tray integration, global shortcuts, the full editor window (preview, tools, timeline, layers, export), the settings dialog, and application packaging — completing the Qt6/C++ frontend migration with all user-facing features wired to the Rust backend through RustBridge FFI.

**Architecture:** Qt6/C++ frontend with a Rust shared library backend. The `RustBridge` class (singleton) wraps all FFI calls to the Rust `libsd_core` library via C-ABI functions. `AppState` (singleton, QObject) manages application-wide state and emits signals on state changes. All UI components are QWidgets/QDialogs that read from AppState and call RustBridge methods. The editor uses a canvas-based approach with overlay rendering for annotations.

**Tech Stack:** Qt 6.5+, C++17, Rust (2021 edition), CMake, FFmpeg (bundled or system), qhotkey (global shortcuts), linuxdeployqt/macdeployqt/NSIS (packaging)

**Depends on:** Plan 5a (Rust core library with C-ABI exports), Plan 5b (Qt skeleton — RustBridge, AppState, MainWindow with capture cards)

**Related documents:**
- `PLAN.md` — high-level architecture and feature overview
- `docs/plans/01-core-platform-infrastructure.md` — Plan 1 (Tauri original)
- `docs/plans/02-screen-capture-recording.md` — Plan 2 (Tauri original)
- `docs/plans/03-media-editing.md` — Plan 3 (Tauri original)
- `docs/plans/04-export-sharing.md` — Plan 4 (Tauri original)

---

## File Structure

```
screen-recorder/
├── qt/
│   ├── CMakeLists.txt                            # (modify) add new sources, qhotkey dep
│   ├── resources/
│   │   ├── icons/
│   │   │   ├── tray-idle.png                     # Tray icon (normal)
│   │   │   ├── tray-recording.png                # Tray icon (recording — red dot)
│   │   │   ├── tool-select.png                   # Tool icons
│   │   │   ├── tool-text.png
│   │   │   ├── tool-rect.png
│   │   │   ├── tool-ellipse.png
│   │   │   ├── tool-arrow.png
│   │   │   ├── tool-blur.png
│   │   │   ├── tool-image.png
│   │   │   ├── tool-crop.png
│   │   │   └── tool-scale.png
│   │   ├── screendream.desktop                   # Linux .desktop file
│   │   └── resources.qrc                         # (modify) register new icons
│   ├── src/
│   │   ├── main.cpp                              # (modify) init tray, global shortcuts
│   │   ├── RustBridge.h / .cpp                   # (existing from 5b)
│   │   ├── AppState.h / .cpp                     # (existing from 5b, modify)
│   │   ├── MainWindow.h / .cpp                   # (existing from 5b, modify)
│   │   ├── SourcePickerDialog.h / .cpp           # Task 17
│   │   ├── RegionSelector.h / .cpp               # Task 18
│   │   ├── RecorderPanel.h / .cpp                # Task 19
│   │   ├── SystemTray.h / .cpp                   # Task 20
│   │   ├── GlobalShortcuts.h / .cpp              # Task 21
│   │   ├── EditorWindow.h / .cpp                 # Task 22
│   │   ├── VideoPreview.h / .cpp                 # Task 23
│   │   ├── ToolPanel.h / .cpp                    # Task 24
│   │   ├── TimelineWidget.h / .cpp               # Task 25
│   │   ├── LayerPanel.h / .cpp                   # Task 26
│   │   ├── ExportDialog.h / .cpp                 # Task 27
│   │   ├── SettingsDialog.h / .cpp               # Task 28
│   │   └── OverlayItem.h / .cpp                  # Shared overlay data model (used by editor)
│   ├── packaging/
│   │   ├── appimage/
│   │   │   └── build-appimage.sh                 # Task 29
│   │   ├── macos/
│   │   │   └── build-dmg.sh                      # Task 29
│   │   ├── windows/
│   │   │   └── installer.nsi                     # Task 29
│   │   └── ffmpeg/
│   │       └── README.md                         # Instructions for bundling FFmpeg
│   └── third_party/
│       └── qhotkey/                              # qhotkey library (submodule or vendored)
```

---

## Phase 5: Recording Flow (Tasks 17–19)

---

## Task 17: SourcePickerDialog

**Files:**
- Create: `qt/src/SourcePickerDialog.h`
- Create: `qt/src/SourcePickerDialog.cpp`
- Modify: `qt/CMakeLists.txt`

- [ ] **Step 1: Create SourcePickerDialog header**

Create `qt/src/SourcePickerDialog.h`:
```cpp
#ifndef SOURCEPICKERDIALOG_H
#define SOURCEPICKERDIALOG_H

#include <QDialog>
#include <QListWidget>
#include <QLabel>
#include <QPushButton>
#include <QHBoxLayout>
#include <QVBoxLayout>
#include <QDialogButtonBox>

// Forward declarations
class RustBridge;

/// Represents a selected capture source returned by the dialog.
struct SelectedSource {
    enum Type { Screen, Window };
    Type type;
    uint32_t id;         // monitor_id or window_id
    QString name;
    uint32_t width;
    uint32_t height;
};

/// Dialog for picking a screen or window to capture.
/// Calls RustBridge::enumerateSources() to populate two side-by-side lists.
class SourcePickerDialog : public QDialog {
    Q_OBJECT

public:
    explicit SourcePickerDialog(QWidget *parent = nullptr);
    ~SourcePickerDialog() override = default;

    /// Returns the selected source after exec() returns QDialog::Accepted.
    SelectedSource selectedSource() const;

private slots:
    void onScreenSelectionChanged();
    void onWindowSelectionChanged();
    void onSelectClicked();

private:
    void populateSources();
    void setupUi();

    QListWidget *m_screenList = nullptr;
    QListWidget *m_windowList = nullptr;
    QLabel *m_waylandNotice = nullptr;
    QPushButton *m_selectButton = nullptr;
    QPushButton *m_cancelButton = nullptr;

    SelectedSource m_selected;
    bool m_hasSelection = false;
};

#endif // SOURCEPICKERDIALOG_H
```

- [ ] **Step 2: Create SourcePickerDialog implementation**

Create `qt/src/SourcePickerDialog.cpp`:
```cpp
#include "SourcePickerDialog.h"
#include "RustBridge.h"

#include <QGroupBox>
#include <QIcon>
#include <QScreen>
#include <QApplication>

SourcePickerDialog::SourcePickerDialog(QWidget *parent)
    : QDialog(parent)
{
    setWindowTitle(tr("Select Capture Source"));
    setMinimumSize(700, 450);
    setupUi();
    populateSources();
}

void SourcePickerDialog::setupUi()
{
    auto *mainLayout = new QVBoxLayout(this);

    // --- Wayland notice (hidden by default) ---
    m_waylandNotice = new QLabel(this);
    m_waylandNotice->setText(
        tr("Window capture is not available on Wayland. "
           "Only full-screen capture is supported. "
           "Use the system portal picker for window selection."));
    m_waylandNotice->setWordWrap(true);
    m_waylandNotice->setStyleSheet(
        "QLabel { background-color: #FFF3CD; color: #856404; "
        "border: 1px solid #FFEEBA; border-radius: 4px; padding: 8px; }");
    m_waylandNotice->setVisible(false);
    mainLayout->addWidget(m_waylandNotice);

    // --- Two side-by-side list panels ---
    auto *listsLayout = new QHBoxLayout();

    // Screens group
    auto *screenGroup = new QGroupBox(tr("Screens"), this);
    auto *screenLayout = new QVBoxLayout(screenGroup);
    m_screenList = new QListWidget(screenGroup);
    m_screenList->setIconSize(QSize(48, 48));
    m_screenList->setSelectionMode(QAbstractItemView::SingleSelection);
    screenLayout->addWidget(m_screenList);
    listsLayout->addWidget(screenGroup);

    // Windows group
    auto *windowGroup = new QGroupBox(tr("Windows"), this);
    auto *windowLayout = new QVBoxLayout(windowGroup);
    m_windowList = new QListWidget(windowGroup);
    m_windowList->setIconSize(QSize(48, 48));
    m_windowList->setSelectionMode(QAbstractItemView::SingleSelection);
    windowLayout->addWidget(m_windowList);
    listsLayout->addWidget(windowGroup);

    mainLayout->addLayout(listsLayout, 1);

    // --- Buttons ---
    auto *buttonLayout = new QHBoxLayout();
    buttonLayout->addStretch();
    m_selectButton = new QPushButton(tr("Select"), this);
    m_selectButton->setEnabled(false);
    m_selectButton->setDefault(true);
    m_cancelButton = new QPushButton(tr("Cancel"), this);
    buttonLayout->addWidget(m_selectButton);
    buttonLayout->addWidget(m_cancelButton);
    mainLayout->addLayout(buttonLayout);

    // --- Connections ---
    connect(m_screenList, &QListWidget::itemSelectionChanged,
            this, &SourcePickerDialog::onScreenSelectionChanged);
    connect(m_windowList, &QListWidget::itemSelectionChanged,
            this, &SourcePickerDialog::onWindowSelectionChanged);
    connect(m_selectButton, &QPushButton::clicked,
            this, &SourcePickerDialog::onSelectClicked);
    connect(m_cancelButton, &QPushButton::clicked,
            this, &QDialog::reject);
}

void SourcePickerDialog::populateSources()
{
    auto &bridge = RustBridge::instance();

    // sd_enumerate_sources returns JSON; RustBridge parses it into a struct.
    // For this implementation, we assume RustBridge provides:
    //   SdAvailableSources enumerateSources()
    // which contains vectors of SdMonitorInfo and SdWindowInfo.

    auto sources = bridge.enumerateSources();

    // --- Populate screens ---
    for (const auto &monitor : sources.monitors) {
        auto *item = new QListWidgetItem(m_screenList);
        QString label = QString("%1\n%2x%3")
            .arg(monitor.friendly_name.isEmpty() ? monitor.name : monitor.friendly_name)
            .arg(monitor.width)
            .arg(monitor.height);
        if (monitor.is_primary) {
            label += tr(" (Primary)");
        }
        item->setText(label);
        item->setIcon(QIcon::fromTheme("video-display",
                       QIcon(":/icons/video-display.png")));
        item->setData(Qt::UserRole, monitor.id);
        item->setData(Qt::UserRole + 1, static_cast<int>(SelectedSource::Screen));
        item->setData(Qt::UserRole + 2, monitor.name);
        item->setData(Qt::UserRole + 3, monitor.width);
        item->setData(Qt::UserRole + 4, monitor.height);
    }

    // --- Wayland notice ---
    if (sources.windows_unavailable) {
        m_waylandNotice->setVisible(true);
        if (!sources.windows_unavailable_reason.isEmpty()) {
            m_waylandNotice->setText(sources.windows_unavailable_reason);
        }
    }

    // --- Populate windows ---
    for (const auto &window : sources.windows) {
        if (window.is_minimized) continue; // skip minimized windows

        auto *item = new QListWidgetItem(m_windowList);
        QString label = QString("%1 — %2\n%3x%4")
            .arg(window.app_name)
            .arg(window.title)
            .arg(window.width)
            .arg(window.height);
        item->setText(label);
        item->setIcon(QIcon::fromTheme("application-x-executable",
                       QIcon(":/icons/application.png")));
        item->setData(Qt::UserRole, window.id);
        item->setData(Qt::UserRole + 1, static_cast<int>(SelectedSource::Window));
        item->setData(Qt::UserRole + 2, window.app_name);
        item->setData(Qt::UserRole + 3, window.width);
        item->setData(Qt::UserRole + 4, window.height);
    }
}

void SourcePickerDialog::onScreenSelectionChanged()
{
    // Deselect windows when a screen is selected
    m_windowList->clearSelection();
    m_selectButton->setEnabled(m_screenList->currentItem() != nullptr);
}

void SourcePickerDialog::onWindowSelectionChanged()
{
    // Deselect screens when a window is selected
    m_screenList->clearSelection();
    m_selectButton->setEnabled(m_windowList->currentItem() != nullptr);
}

void SourcePickerDialog::onSelectClicked()
{
    QListWidgetItem *item = nullptr;

    if (m_screenList->currentItem()) {
        item = m_screenList->currentItem();
    } else if (m_windowList->currentItem()) {
        item = m_windowList->currentItem();
    }

    if (!item) return;

    m_selected.id = item->data(Qt::UserRole).toUInt();
    m_selected.type = static_cast<SelectedSource::Type>(
        item->data(Qt::UserRole + 1).toInt());
    m_selected.name = item->data(Qt::UserRole + 2).toString();
    m_selected.width = item->data(Qt::UserRole + 3).toUInt();
    m_selected.height = item->data(Qt::UserRole + 4).toUInt();
    m_hasSelection = true;

    accept();
}

SelectedSource SourcePickerDialog::selectedSource() const
{
    return m_selected;
}
```

- [ ] **Step 3: Add to CMakeLists.txt**

Add to the `SOURCES` list in `qt/CMakeLists.txt`:
```cmake
src/SourcePickerDialog.h
src/SourcePickerDialog.cpp
```

- [ ] **Step 4: Verify compilation**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/qt
cmake --build build --target ScreenDream 2>&1 | tail -20
```

---

## Task 18: RegionSelector

**Files:**
- Create: `qt/src/RegionSelector.h`
- Create: `qt/src/RegionSelector.cpp`
- Modify: `qt/CMakeLists.txt`

- [ ] **Step 1: Create RegionSelector header**

Create `qt/src/RegionSelector.h`:
```cpp
#ifndef REGIONSELECTOR_H
#define REGIONSELECTOR_H

#include <QWidget>
#include <QRect>
#include <QPoint>
#include <QLabel>

/// Fullscreen transparent overlay for drawing a region selection rectangle.
///
/// Usage:
///   auto *selector = new RegionSelector(targetMonitorGeometry);
///   connect(selector, &RegionSelector::regionSelected, this, &MyClass::onRegion);
///   selector->show();
///
/// The widget covers the target monitor, shows a crosshair cursor, and lets the
/// user drag a rectangle. On release, emits regionSelected(QRect). Escape cancels.
class RegionSelector : public QWidget {
    Q_OBJECT

public:
    /// @param monitorGeometry The geometry of the target monitor in global coords.
    /// @param parent Parent widget (usually nullptr for top-level).
    explicit RegionSelector(const QRect &monitorGeometry, QWidget *parent = nullptr);
    ~RegionSelector() override = default;

signals:
    /// Emitted when the user finishes dragging a selection rectangle.
    /// @param region The selected rectangle in screen coordinates (global).
    void regionSelected(QRect region);

    /// Emitted when the user cancels (Escape key).
    void cancelled();

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;

private:
    QRect normalizedSelection() const;

    QRect m_monitorGeometry;
    QPoint m_startPos;
    QPoint m_currentPos;
    bool m_selecting = false;
    bool m_hasSelection = false;

    QLabel *m_dimensionLabel = nullptr;
};

#endif // REGIONSELECTOR_H
```

- [ ] **Step 2: Create RegionSelector implementation**

Create `qt/src/RegionSelector.cpp`:
```cpp
#include "RegionSelector.h"

#include <QPainter>
#include <QMouseEvent>
#include <QKeyEvent>
#include <QGuiApplication>
#include <QScreen>

RegionSelector::RegionSelector(const QRect &monitorGeometry, QWidget *parent)
    : QWidget(parent)
    , m_monitorGeometry(monitorGeometry)
{
    // Frameless, always on top, translucent background
    setWindowFlags(Qt::FramelessWindowHint
                 | Qt::WindowStaysOnTopHint
                 | Qt::Tool);
    setAttribute(Qt::WA_TranslucentBackground);
    setAttribute(Qt::WA_DeleteOnClose);

    // Cover the target monitor
    setGeometry(monitorGeometry);

    // Crosshair cursor
    setCursor(Qt::CrossCursor);

    // Dimension label (floating, updated during drag)
    m_dimensionLabel = new QLabel(this);
    m_dimensionLabel->setStyleSheet(
        "QLabel {"
        "  background-color: rgba(0, 0, 0, 180);"
        "  color: #FFFFFF;"
        "  font-size: 13px;"
        "  font-weight: bold;"
        "  padding: 4px 8px;"
        "  border-radius: 3px;"
        "}");
    m_dimensionLabel->setVisible(false);

    // Grab keyboard focus
    setFocusPolicy(Qt::StrongFocus);
    setFocus();
    setMouseTracking(true);

    showFullScreen();
}

void RegionSelector::paintEvent(QPaintEvent * /*event*/)
{
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);

    // Subtle darkened overlay — 5% opacity black over entire screen
    painter.fillRect(rect(), QColor(0, 0, 0, 13)); // 255 * 0.05 ≈ 13

    if (m_selecting || m_hasSelection) {
        QRect sel = normalizedSelection();

        // Clear the selection area (punch through the overlay)
        painter.setCompositionMode(QPainter::CompositionMode_Clear);
        painter.fillRect(sel, Qt::transparent);
        painter.setCompositionMode(QPainter::CompositionMode_SourceOver);

        // Re-apply the very subtle overlay on the cleared area so it matches
        // (the selection area is fully clear; we want it brighter than surroundings)

        // Draw accent border around selection
        QPen borderPen(QColor(0, 120, 255), 2, Qt::SolidLine);
        painter.setPen(borderPen);
        painter.setBrush(Qt::NoBrush);
        painter.drawRect(sel);

        // Corner handles (small squares at corners)
        const int handleSize = 6;
        painter.setBrush(QColor(0, 120, 255));
        painter.setPen(Qt::NoPen);
        QPoint corners[] = {
            sel.topLeft(), sel.topRight(),
            sel.bottomLeft(), sel.bottomRight()
        };
        for (const auto &corner : corners) {
            painter.drawRect(QRect(
                corner.x() - handleSize / 2,
                corner.y() - handleSize / 2,
                handleSize, handleSize));
        }
    }
}

void RegionSelector::mousePressEvent(QMouseEvent *event)
{
    if (event->button() == Qt::LeftButton) {
        m_startPos = event->pos();
        m_currentPos = event->pos();
        m_selecting = true;
        m_hasSelection = false;
        m_dimensionLabel->setVisible(true);
        update();
    }
}

void RegionSelector::mouseMoveEvent(QMouseEvent *event)
{
    if (m_selecting) {
        m_currentPos = event->pos();

        // Update dimension label
        QRect sel = normalizedSelection();
        m_dimensionLabel->setText(
            QString("%1 x %2").arg(sel.width()).arg(sel.height()));
        m_dimensionLabel->adjustSize();

        // Position the label near the bottom-right of the selection
        int labelX = sel.right() + 8;
        int labelY = sel.bottom() + 8;

        // Keep label on screen
        if (labelX + m_dimensionLabel->width() > width()) {
            labelX = sel.right() - m_dimensionLabel->width() - 8;
        }
        if (labelY + m_dimensionLabel->height() > height()) {
            labelY = sel.bottom() - m_dimensionLabel->height() - 8;
        }

        m_dimensionLabel->move(labelX, labelY);
        update();
    }
}

void RegionSelector::mouseReleaseEvent(QMouseEvent *event)
{
    if (event->button() == Qt::LeftButton && m_selecting) {
        m_selecting = false;
        m_currentPos = event->pos();

        QRect sel = normalizedSelection();

        // Ignore tiny selections (accidental clicks)
        if (sel.width() < 10 || sel.height() < 10) {
            m_hasSelection = false;
            m_dimensionLabel->setVisible(false);
            update();
            return;
        }

        m_hasSelection = true;
        update();

        // Convert to global (screen) coordinates
        QRect globalSel(
            sel.x() + m_monitorGeometry.x(),
            sel.y() + m_monitorGeometry.y(),
            sel.width(),
            sel.height());

        emit regionSelected(globalSel);
        close();
    }
}

void RegionSelector::keyPressEvent(QKeyEvent *event)
{
    if (event->key() == Qt::Key_Escape) {
        emit cancelled();
        close();
    }
}

QRect RegionSelector::normalizedSelection() const
{
    return QRect(
        QPoint(qMin(m_startPos.x(), m_currentPos.x()),
               qMin(m_startPos.y(), m_currentPos.y())),
        QPoint(qMax(m_startPos.x(), m_currentPos.x()),
               qMax(m_startPos.y(), m_currentPos.y()))
    );
}
```

- [ ] **Step 3: Add to CMakeLists.txt**

Add to the `SOURCES` list in `qt/CMakeLists.txt`:
```cmake
src/RegionSelector.h
src/RegionSelector.cpp
```

- [ ] **Step 4: Verify compilation**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/qt
cmake --build build --target ScreenDream 2>&1 | tail -20
```

---

## Task 19: RecorderPanel (floating mini control)

**Files:**
- Create: `qt/src/RecorderPanel.h`
- Create: `qt/src/RecorderPanel.cpp`
- Modify: `qt/CMakeLists.txt`

- [ ] **Step 1: Create RecorderPanel header**

Create `qt/src/RecorderPanel.h`:
```cpp
#ifndef RECORDERPANEL_H
#define RECORDERPANEL_H

#include <QWidget>
#include <QLabel>
#include <QPushButton>
#include <QTimer>
#include <QElapsedTimer>
#include <QPoint>
#include <QPropertyAnimation>

/// Floating mini control panel shown during recording.
/// Displays recording status, timer, preview thumbnail, and stop/pause controls.
///
/// Positioned at bottom-right of the screen. Draggable by the title area.
class RecorderPanel : public QWidget {
    Q_OBJECT
    Q_PROPERTY(qreal dotOpacity READ dotOpacity WRITE setDotOpacity)

public:
    explicit RecorderPanel(QWidget *parent = nullptr);
    ~RecorderPanel() override = default;

    /// Start the recording timer and pulsing animation.
    void startRecording(const QString &outputPath, int fps, const QString &resolution);

    /// Pause the timer display.
    void pauseRecording();

    /// Resume the timer display.
    void resumeRecording();

    /// Stop everything and prepare for close.
    void stopRecording();

    qreal dotOpacity() const { return m_dotOpacity; }
    void setDotOpacity(qreal opacity);

signals:
    /// Emitted when the user clicks the Pause/Resume button.
    void pauseResumeClicked();

    /// Emitted when the user clicks the Stop button.
    void stopClicked();

    /// Emitted after recording stops, with the output file path.
    void recordingStopped(QString outputPath);

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;

private slots:
    void updateTimer();
    void updatePreviewThumbnail();

private:
    void setupUi();
    void positionAtBottomRight();
    void startPulsingDot();
    void stopPulsingDot();

    // UI elements
    QLabel *m_dotLabel = nullptr;
    QLabel *m_recLabel = nullptr;
    QLabel *m_timerLabel = nullptr;
    QPushButton *m_pauseButton = nullptr;
    QPushButton *m_stopButton = nullptr;
    QLabel *m_previewLabel = nullptr;
    QLabel *m_statusLine = nullptr;

    // Timer
    QTimer *m_timerTick = nullptr;
    QElapsedTimer m_elapsed;
    qint64 m_pausedElapsed = 0;
    bool m_isPaused = false;

    // Preview thumbnail refresh
    QTimer *m_previewTimer = nullptr;

    // Pulsing dot animation
    QPropertyAnimation *m_dotAnimation = nullptr;
    qreal m_dotOpacity = 1.0;

    // Dragging
    QPoint m_dragStartPos;
    bool m_dragging = false;

    // Recording info
    QString m_outputPath;
    int m_fps = 0;
    QString m_resolution;

    // Title bar height for drag area
    static constexpr int kTitleBarHeight = 24;
};

#endif // RECORDERPANEL_H
```

- [ ] **Step 2: Create RecorderPanel implementation**

Create `qt/src/RecorderPanel.cpp`:
```cpp
#include "RecorderPanel.h"
#include "RustBridge.h"

#include <QHBoxLayout>
#include <QVBoxLayout>
#include <QPainter>
#include <QMouseEvent>
#include <QScreen>
#include <QGuiApplication>
#include <QStyle>
#include <QPixmap>
#include <QImage>

RecorderPanel::RecorderPanel(QWidget *parent)
    : QWidget(parent)
{
    setWindowFlags(Qt::FramelessWindowHint
                 | Qt::WindowStaysOnTopHint
                 | Qt::Tool);
    setAttribute(Qt::WA_DeleteOnClose, false); // reusable
    setFixedSize(320, 80);

    setupUi();
    positionAtBottomRight();
}

void RecorderPanel::setupUi()
{
    auto *mainLayout = new QVBoxLayout(this);
    mainLayout->setContentsMargins(8, kTitleBarHeight + 4, 8, 6);
    mainLayout->setSpacing(4);

    // --- Top row: dot + REC + timer + buttons + preview ---
    auto *topRow = new QHBoxLayout();
    topRow->setSpacing(6);

    // Pulsing red dot (painted in paintEvent, this label is for layout spacing)
    m_dotLabel = new QLabel(this);
    m_dotLabel->setFixedSize(12, 12);
    topRow->addWidget(m_dotLabel);

    // "REC" label
    m_recLabel = new QLabel(tr("REC"), this);
    m_recLabel->setStyleSheet("QLabel { color: #FF3B30; font-weight: bold; font-size: 13px; }");
    topRow->addWidget(m_recLabel);

    // Timer label
    m_timerLabel = new QLabel("00:00", this);
    m_timerLabel->setStyleSheet("QLabel { color: #FFFFFF; font-size: 14px; font-family: monospace; }");
    m_timerLabel->setMinimumWidth(50);
    topRow->addWidget(m_timerLabel);

    topRow->addStretch();

    // Pause/Resume button
    m_pauseButton = new QPushButton(tr("⏸"), this);
    m_pauseButton->setFixedSize(28, 28);
    m_pauseButton->setToolTip(tr("Pause/Resume"));
    connect(m_pauseButton, &QPushButton::clicked, this, &RecorderPanel::pauseResumeClicked);
    topRow->addWidget(m_pauseButton);

    // Stop button
    m_stopButton = new QPushButton(tr("⏹"), this);
    m_stopButton->setFixedSize(28, 28);
    m_stopButton->setToolTip(tr("Stop Recording"));
    m_stopButton->setStyleSheet("QPushButton { background-color: #FF3B30; color: white; border-radius: 4px; }");
    connect(m_stopButton, &QPushButton::clicked, this, &RecorderPanel::stopClicked);
    topRow->addWidget(m_stopButton);

    // Preview thumbnail
    m_previewLabel = new QLabel(this);
    m_previewLabel->setFixedSize(48, 28);
    m_previewLabel->setStyleSheet("QLabel { background-color: #333; border: 1px solid #555; }");
    m_previewLabel->setScaledContents(true);
    topRow->addWidget(m_previewLabel);

    mainLayout->addLayout(topRow);

    // --- Status line ---
    m_statusLine = new QLabel(this);
    m_statusLine->setStyleSheet("QLabel { color: #999; font-size: 10px; }");
    mainLayout->addWidget(m_statusLine);

    // --- Timers ---
    m_timerTick = new QTimer(this);
    m_timerTick->setInterval(1000);
    connect(m_timerTick, &QTimer::timeout, this, &RecorderPanel::updateTimer);

    m_previewTimer = new QTimer(this);
    m_previewTimer->setInterval(2000); // update thumbnail every 2s
    connect(m_previewTimer, &QTimer::timeout, this, &RecorderPanel::updatePreviewThumbnail);

    // Style the panel background
    setStyleSheet(
        "RecorderPanel {"
        "  background-color: rgba(30, 30, 30, 230);"
        "  border: 1px solid #555;"
        "  border-radius: 8px;"
        "}");
}

void RecorderPanel::positionAtBottomRight()
{
    QScreen *screen = QGuiApplication::primaryScreen();
    if (!screen) return;

    QRect geo = screen->availableGeometry();
    int x = geo.right() - width() - 20;
    int y = geo.bottom() - height() - 20;
    move(x, y);
}

void RecorderPanel::startRecording(const QString &outputPath, int fps, const QString &resolution)
{
    m_outputPath = outputPath;
    m_fps = fps;
    m_resolution = resolution;
    m_pausedElapsed = 0;
    m_isPaused = false;

    m_statusLine->setText(
        QString("%1 FPS | %2 | %3").arg(fps).arg(resolution).arg(outputPath));

    m_elapsed.start();
    m_timerTick->start();
    m_previewTimer->start();
    startPulsingDot();

    m_pauseButton->setText(tr("⏸"));
    show();
}

void RecorderPanel::pauseRecording()
{
    m_isPaused = true;
    m_pausedElapsed += m_elapsed.elapsed();
    m_timerTick->stop();
    stopPulsingDot();
    m_pauseButton->setText(tr("▶"));
    m_recLabel->setText(tr("PAUSED"));
    m_recLabel->setStyleSheet("QLabel { color: #FFD60A; font-weight: bold; font-size: 13px; }");
}

void RecorderPanel::resumeRecording()
{
    m_isPaused = false;
    m_elapsed.restart();
    m_timerTick->start();
    startPulsingDot();
    m_pauseButton->setText(tr("⏸"));
    m_recLabel->setText(tr("REC"));
    m_recLabel->setStyleSheet("QLabel { color: #FF3B30; font-weight: bold; font-size: 13px; }");
}

void RecorderPanel::stopRecording()
{
    m_timerTick->stop();
    m_previewTimer->stop();
    stopPulsingDot();
    emit recordingStopped(m_outputPath);
    hide();
}

void RecorderPanel::updateTimer()
{
    qint64 totalMs = m_pausedElapsed + m_elapsed.elapsed();
    int totalSeconds = static_cast<int>(totalMs / 1000);
    int minutes = totalSeconds / 60;
    int seconds = totalSeconds % 60;
    m_timerLabel->setText(
        QString("%1:%2")
            .arg(minutes, 2, 10, QChar('0'))
            .arg(seconds, 2, 10, QChar('0')));
}

void RecorderPanel::updatePreviewThumbnail()
{
    // Capture a frame via RustBridge for the thumbnail preview
    auto &bridge = RustBridge::instance();
    QImage frame = bridge.capturePreviewFrame();
    if (!frame.isNull()) {
        m_previewLabel->setPixmap(
            QPixmap::fromImage(frame).scaled(
                m_previewLabel->size(), Qt::KeepAspectRatio, Qt::SmoothTransformation));
    }
}

void RecorderPanel::startPulsingDot()
{
    if (!m_dotAnimation) {
        m_dotAnimation = new QPropertyAnimation(this, "dotOpacity", this);
        m_dotAnimation->setDuration(1000);
        m_dotAnimation->setStartValue(1.0);
        m_dotAnimation->setEndValue(0.2);
        m_dotAnimation->setLoopCount(-1); // infinite
        m_dotAnimation->setEasingCurve(QEasingCurve::InOutSine);
    }
    // Make it ping-pong by setting the direction to alternate
    // QPropertyAnimation doesn't have auto-reverse, so we use keyframes
    m_dotAnimation->setKeyValueAt(0.0, 1.0);
    m_dotAnimation->setKeyValueAt(0.5, 0.2);
    m_dotAnimation->setKeyValueAt(1.0, 1.0);
    m_dotAnimation->start();
}

void RecorderPanel::stopPulsingDot()
{
    if (m_dotAnimation) {
        m_dotAnimation->stop();
    }
    m_dotOpacity = 1.0;
    update();
}

void RecorderPanel::setDotOpacity(qreal opacity)
{
    m_dotOpacity = opacity;
    update(); // trigger repaint for the dot
}

void RecorderPanel::paintEvent(QPaintEvent *event)
{
    QWidget::paintEvent(event);
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);

    // Draw draggable title bar area
    painter.setPen(Qt::NoPen);
    painter.setBrush(QColor(50, 50, 50, 200));
    painter.drawRoundedRect(0, 0, width(), kTitleBarHeight, 8, 8);

    // Draw drag grip dots in title bar
    painter.setBrush(QColor(120, 120, 120));
    for (int i = 0; i < 3; ++i) {
        int dotX = width() / 2 - 10 + i * 10;
        int dotY = kTitleBarHeight / 2;
        painter.drawEllipse(QPoint(dotX, dotY), 2, 2);
    }

    // Draw pulsing red dot at the position of m_dotLabel
    if (m_dotLabel) {
        QPoint dotCenter = m_dotLabel->geometry().center();
        painter.setBrush(QColor(255, 59, 48, static_cast<int>(m_dotOpacity * 255)));
        painter.setPen(Qt::NoPen);
        painter.drawEllipse(dotCenter, 5, 5);
    }
}

void RecorderPanel::mousePressEvent(QMouseEvent *event)
{
    if (event->button() == Qt::LeftButton && event->pos().y() <= kTitleBarHeight) {
        m_dragging = true;
        m_dragStartPos = event->globalPosition().toPoint() - frameGeometry().topLeft();
        event->accept();
    }
}

void RecorderPanel::mouseMoveEvent(QMouseEvent *event)
{
    if (m_dragging) {
        move(event->globalPosition().toPoint() - m_dragStartPos);
        event->accept();
    }
}

void RecorderPanel::mouseReleaseEvent(QMouseEvent *event)
{
    if (event->button() == Qt::LeftButton) {
        m_dragging = false;
        event->accept();
    }
}
```

- [ ] **Step 3: Add to CMakeLists.txt**

Add to the `SOURCES` list in `qt/CMakeLists.txt`:
```cmake
src/RecorderPanel.h
src/RecorderPanel.cpp
```

- [ ] **Step 4: Wire recording flow in MainWindow**

In `qt/src/MainWindow.cpp`, add the recording flow that connects source picker, region selector, and recorder panel:

```cpp
// Add to MainWindow.h private members:
#include "SourcePickerDialog.h"
#include "RegionSelector.h"
#include "RecorderPanel.h"

// ...
private:
    RecorderPanel *m_recorderPanel = nullptr;

private slots:
    void onStartRecording();
    void onRecordingStopped(const QString &outputPath);
```

```cpp
// Add to MainWindow.cpp:

void MainWindow::onStartRecording()
{
    // Step 1: Show source picker
    SourcePickerDialog picker(this);
    if (picker.exec() != QDialog::Accepted) return;

    SelectedSource source = picker.selectedSource();

    // Step 2: If screen selected, optionally show region selector
    // (In a full implementation, a "Region" button in the picker triggers this)

    // Step 3: Build recording config and start via RustBridge
    auto &bridge = RustBridge::instance();
    auto &state = AppState::instance();

    QString outputDir = state.settings().export_settings.output_directory;
    QString timestamp = QDateTime::currentDateTime().toString("yyyy-MM-dd_HH-mm-ss");
    QString outputPath = QString("%1/Screen Dream %2.mp4").arg(outputDir, timestamp);

    // Construct FFI config and call sd_start_recording
    bool ok = bridge.startRecording(
        source.type == SelectedSource::Screen ? "screen" : "window",
        source.id,
        state.settings().recording.fps,
        state.settings().recording.crf,
        outputPath);

    if (!ok) {
        qWarning() << "Failed to start recording";
        return;
    }

    // Step 4: Show recorder panel
    if (!m_recorderPanel) {
        m_recorderPanel = new RecorderPanel(nullptr); // top-level widget
        connect(m_recorderPanel, &RecorderPanel::stopClicked, this, [this]() {
            RustBridge::instance().stopRecording();
            m_recorderPanel->stopRecording();
        });
        connect(m_recorderPanel, &RecorderPanel::pauseResumeClicked, this, [this]() {
            auto &bridge = RustBridge::instance();
            auto &state = AppState::instance();
            if (state.recordingState() == "paused") {
                bridge.resumeRecording();
                m_recorderPanel->resumeRecording();
            } else {
                bridge.pauseRecording();
                m_recorderPanel->pauseRecording();
            }
        });
        connect(m_recorderPanel, &RecorderPanel::recordingStopped,
                this, &MainWindow::onRecordingStopped);
    }

    QString resolution = QString("%1x%2").arg(source.width).arg(source.height);
    m_recorderPanel->startRecording(
        outputPath, state.settings().recording.fps, resolution);

    // Optionally minimize main window
    if (state.settings().general.minimize_to_tray) {
        hide();
    }
}

void MainWindow::onRecordingStopped(const QString &outputPath)
{
    show();
    raise();
    activateWindow();

    // Refresh recent captures list
    AppState::instance().refreshRecentCaptures();

    // Optionally open the editor
    // EditorWindow *editor = new EditorWindow(outputPath, this);
    // editor->show();
}
```

- [ ] **Step 5: Verify compilation**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/qt
cmake --build build --target ScreenDream 2>&1 | tail -20
```

---

## Phase 6: System Integration (Tasks 20–21)

---

## Task 20: System Tray

**Files:**
- Create: `qt/src/SystemTray.h`
- Create: `qt/src/SystemTray.cpp`
- Modify: `qt/CMakeLists.txt`
- Modify: `qt/src/main.cpp`

- [ ] **Step 1: Create SystemTray header**

Create `qt/src/SystemTray.h`:
```cpp
#ifndef SYSTEMTRAY_H
#define SYSTEMTRAY_H

#include <QSystemTrayIcon>
#include <QMenu>
#include <QAction>

class MainWindow;

/// System tray icon with context menu for quick recording control.
///
/// Shows different icon when recording (red dot overlay).
/// Menu items: Show Window, Start/Stop Recording, Take Screenshot, Quit.
class SystemTray : public QObject {
    Q_OBJECT

public:
    explicit SystemTray(MainWindow *mainWindow, QObject *parent = nullptr);
    ~SystemTray() override;

    /// Update the tray icon to reflect recording state.
    void setRecording(bool recording);

    /// Returns true if system tray is available on this platform.
    static bool isAvailable();

signals:
    void showWindowRequested();
    void startStopRecordingRequested();
    void takeScreenshotRequested();
    void quitRequested();

private slots:
    void onTrayActivated(QSystemTrayIcon::ActivationReason reason);
    void updateMenu();

private:
    void createMenu();
    QIcon createRecordingIcon();

    QSystemTrayIcon *m_trayIcon = nullptr;
    QMenu *m_trayMenu = nullptr;

    QAction *m_showAction = nullptr;
    QAction *m_recordAction = nullptr;
    QAction *m_screenshotAction = nullptr;
    QAction *m_quitAction = nullptr;

    QIcon m_idleIcon;
    QIcon m_recordingIcon;
    bool m_isRecording = false;

    MainWindow *m_mainWindow = nullptr;
};

#endif // SYSTEMTRAY_H
```

- [ ] **Step 2: Create SystemTray implementation**

Create `qt/src/SystemTray.cpp`:
```cpp
#include "SystemTray.h"
#include "MainWindow.h"
#include "AppState.h"

#include <QApplication>
#include <QPainter>
#include <QPixmap>

SystemTray::SystemTray(MainWindow *mainWindow, QObject *parent)
    : QObject(parent)
    , m_mainWindow(mainWindow)
{
    // Load icons
    m_idleIcon = QIcon(":/icons/tray-idle.png");
    m_recordingIcon = createRecordingIcon();

    // Create tray icon
    m_trayIcon = new QSystemTrayIcon(m_idleIcon, this);
    m_trayIcon->setToolTip(tr("Screen Dream"));

    createMenu();
    m_trayIcon->setContextMenu(m_trayMenu);

    connect(m_trayIcon, &QSystemTrayIcon::activated,
            this, &SystemTray::onTrayActivated);

    // Listen for recording state changes to update icon/menu
    connect(&AppState::instance(), &AppState::recordingStateChanged,
            this, &SystemTray::updateMenu);

    m_trayIcon->show();
}

SystemTray::~SystemTray()
{
    m_trayIcon->hide();
}

bool SystemTray::isAvailable()
{
    return QSystemTrayIcon::isSystemTrayAvailable();
}

void SystemTray::createMenu()
{
    m_trayMenu = new QMenu();

    m_showAction = m_trayMenu->addAction(tr("Show Window"));
    connect(m_showAction, &QAction::triggered, this, &SystemTray::showWindowRequested);

    m_trayMenu->addSeparator();

    m_recordAction = m_trayMenu->addAction(tr("Start Recording"));
    connect(m_recordAction, &QAction::triggered,
            this, &SystemTray::startStopRecordingRequested);

    m_screenshotAction = m_trayMenu->addAction(tr("Take Screenshot"));
    connect(m_screenshotAction, &QAction::triggered,
            this, &SystemTray::takeScreenshotRequested);

    m_trayMenu->addSeparator();

    m_quitAction = m_trayMenu->addAction(tr("Quit"));
    connect(m_quitAction, &QAction::triggered,
            this, &SystemTray::quitRequested);
}

void SystemTray::setRecording(bool recording)
{
    m_isRecording = recording;
    m_trayIcon->setIcon(recording ? m_recordingIcon : m_idleIcon);
    m_recordAction->setText(recording ? tr("Stop Recording") : tr("Start Recording"));
}

void SystemTray::updateMenu()
{
    auto &state = AppState::instance();
    bool recording = (state.recordingState() == "recording"
                   || state.recordingState() == "paused");
    setRecording(recording);
}

void SystemTray::onTrayActivated(QSystemTrayIcon::ActivationReason reason)
{
    switch (reason) {
    case QSystemTrayIcon::DoubleClick:
    case QSystemTrayIcon::Trigger:
        emit showWindowRequested();
        break;
    default:
        break;
    }
}

QIcon SystemTray::createRecordingIcon()
{
    // Create a recording icon by overlaying a red dot on the idle icon
    QPixmap base = m_idleIcon.pixmap(64, 64);
    if (base.isNull()) {
        // Fallback: create a simple icon with a red dot
        base = QPixmap(64, 64);
        base.fill(Qt::transparent);
    }

    QPainter painter(&base);
    painter.setRenderHint(QPainter::Antialiasing);

    // Red dot in bottom-right corner
    painter.setBrush(QColor(255, 59, 48));
    painter.setPen(QPen(QColor(30, 30, 30), 2));
    painter.drawEllipse(QPoint(52, 52), 10, 10);
    painter.end();

    return QIcon(base);
}
```

- [ ] **Step 3: Integrate SystemTray in main.cpp**

Add to `qt/src/main.cpp`:
```cpp
#include "SystemTray.h"
#include "MainWindow.h"
#include "AppState.h"

// In main(), after creating MainWindow:
MainWindow mainWindow;

SystemTray *tray = nullptr;
if (SystemTray::isAvailable()) {
    tray = new SystemTray(&mainWindow);

    QObject::connect(tray, &SystemTray::showWindowRequested, &mainWindow, [&mainWindow]() {
        mainWindow.show();
        mainWindow.raise();
        mainWindow.activateWindow();
    });

    QObject::connect(tray, &SystemTray::startStopRecordingRequested,
                     &mainWindow, &MainWindow::onStartRecording);

    QObject::connect(tray, &SystemTray::takeScreenshotRequested,
                     &mainWindow, &MainWindow::onTakeScreenshot);

    QObject::connect(tray, &SystemTray::quitRequested, &app, &QApplication::quit);
}

// Override close behavior: minimize to tray instead of quitting
// (handled in MainWindow::closeEvent if setting is enabled)
```

- [ ] **Step 4: Handle close-to-tray in MainWindow**

Add to `qt/src/MainWindow.cpp`:
```cpp
#include <QCloseEvent>

void MainWindow::closeEvent(QCloseEvent *event)
{
    if (AppState::instance().settings().general.minimize_to_tray
        && SystemTray::isAvailable()) {
        hide();
        event->ignore(); // don't actually close
    } else {
        event->accept();
    }
}
```

- [ ] **Step 5: Add to CMakeLists.txt and verify**

Add to the `SOURCES` list:
```cmake
src/SystemTray.h
src/SystemTray.cpp
```

```bash
cd /home/rw3iss/Sites/others/screen-recorder/qt
cmake --build build --target ScreenDream 2>&1 | tail -20
```

---

## Task 21: Global Shortcuts

**Files:**
- Create: `qt/src/GlobalShortcuts.h`
- Create: `qt/src/GlobalShortcuts.cpp`
- Add: `qt/third_party/qhotkey/` (submodule or vendored)
- Modify: `qt/CMakeLists.txt`
- Modify: `qt/src/main.cpp`

- [ ] **Step 1: Add qhotkey dependency**

Option A — Git submodule:
```bash
cd /home/rw3iss/Sites/others/screen-recorder/qt/third_party
git submodule add https://github.com/Skycoder42/QHotkey.git qhotkey
```

Option B — Vendored: download and extract QHotkey source into `qt/third_party/qhotkey/`.

Add to `qt/CMakeLists.txt`:
```cmake
# --- qhotkey (global hotkeys) ---
add_subdirectory(third_party/qhotkey)
target_link_libraries(ScreenDream PRIVATE qhotkey)
```

- [ ] **Step 2: Create GlobalShortcuts header**

Create `qt/src/GlobalShortcuts.h`:
```cpp
#ifndef GLOBALSHORTCUTS_H
#define GLOBALSHORTCUTS_H

#include <QObject>
#include <QKeySequence>
#include <QMap>
#include <memory>

class QHotkey;

/// Manages global keyboard shortcuts that work even when the app is not focused.
///
/// Uses the qhotkey library for cross-platform global hotkey registration.
/// On Wayland, global shortcuts may require the org.freedesktop.GlobalShortcuts
/// portal; qhotkey handles the platform abstraction.
///
/// Shortcuts are loaded from AppState settings and can be re-registered
/// when settings change.
class GlobalShortcuts : public QObject {
    Q_OBJECT

public:
    explicit GlobalShortcuts(QObject *parent = nullptr);
    ~GlobalShortcuts() override;

    /// Register all shortcuts from current settings.
    void registerAll();

    /// Unregister all shortcuts (e.g., before re-registering after settings change).
    void unregisterAll();

    /// Re-read settings and re-register shortcuts.
    void reloadFromSettings();

    /// Check if global shortcuts are supported on this platform.
    static bool isSupported();

signals:
    /// Emitted when the start/stop recording shortcut is triggered.
    void startStopRecordingTriggered();

    /// Emitted when the pause/resume recording shortcut is triggered.
    void pauseResumeRecordingTriggered();

    /// Emitted when the screenshot shortcut is triggered.
    void takeScreenshotTriggered();

private:
    struct RegisteredShortcut {
        QHotkey *hotkey = nullptr;
        QKeySequence sequence;
    };

    void registerShortcut(const QString &id, const QKeySequence &seq,
                          const char *signal);

    QMap<QString, RegisteredShortcut> m_shortcuts;
};

#endif // GLOBALSHORTCUTS_H
```

- [ ] **Step 3: Create GlobalShortcuts implementation**

Create `qt/src/GlobalShortcuts.cpp`:
```cpp
#include "GlobalShortcuts.h"
#include "AppState.h"

#include <QHotkey>
#include <QKeySequence>
#include <QDebug>

GlobalShortcuts::GlobalShortcuts(QObject *parent)
    : QObject(parent)
{
}

GlobalShortcuts::~GlobalShortcuts()
{
    unregisterAll();
}

bool GlobalShortcuts::isSupported()
{
    // QHotkey supports X11, macOS, and Windows.
    // On pure Wayland without XWayland, it may not work.
    // Check at runtime.
    return QHotkey::isPlatformSupported();
}

void GlobalShortcuts::registerAll()
{
    auto &state = AppState::instance();
    const auto &shortcuts = state.settings().shortcuts;

    // Parse shortcut strings from settings.
    // Settings use Electron-style format "CommandOrControl+Shift+R";
    // convert to Qt format "Ctrl+Shift+R".
    auto parseKey = [](const QString &shortcutStr) -> QKeySequence {
        QString qtStr = shortcutStr;
        qtStr.replace("CommandOrControl", "Ctrl");
        qtStr.replace("Command", "Meta");
        qtStr.replace("Control", "Ctrl");
        return QKeySequence(qtStr);
    };

    // Start/Stop Recording
    QKeySequence startStopSeq = parseKey(shortcuts.start_stop_recording);
    if (!startStopSeq.isEmpty()) {
        registerShortcut("start_stop_recording", startStopSeq,
                         SIGNAL(startStopRecordingTriggered()));
    }

    // Pause/Resume Recording
    QKeySequence pauseResumeSeq = parseKey(shortcuts.pause_resume_recording);
    if (!pauseResumeSeq.isEmpty()) {
        registerShortcut("pause_resume_recording", pauseResumeSeq,
                         SIGNAL(pauseResumeRecordingTriggered()));
    }

    // Take Screenshot
    QKeySequence screenshotSeq = parseKey(shortcuts.take_screenshot);
    if (!screenshotSeq.isEmpty()) {
        registerShortcut("take_screenshot", screenshotSeq,
                         SIGNAL(takeScreenshotTriggered()));
    }
}

void GlobalShortcuts::unregisterAll()
{
    for (auto &entry : m_shortcuts) {
        if (entry.hotkey) {
            entry.hotkey->setRegistered(false);
            delete entry.hotkey;
            entry.hotkey = nullptr;
        }
    }
    m_shortcuts.clear();
}

void GlobalShortcuts::reloadFromSettings()
{
    unregisterAll();
    registerAll();
}

void GlobalShortcuts::registerShortcut(const QString &id,
                                        const QKeySequence &seq,
                                        const char *signal)
{
    auto *hotkey = new QHotkey(seq, true, this);

    if (!hotkey->isRegistered()) {
        qWarning() << "Failed to register global shortcut:" << id
                    << "(" << seq.toString() << ")";
        delete hotkey;
        return;
    }

    // Connect the activated signal to the appropriate signal of this class
    if (id == "start_stop_recording") {
        connect(hotkey, &QHotkey::activated, this, &GlobalShortcuts::startStopRecordingTriggered);
    } else if (id == "pause_resume_recording") {
        connect(hotkey, &QHotkey::activated, this, &GlobalShortcuts::pauseResumeRecordingTriggered);
    } else if (id == "take_screenshot") {
        connect(hotkey, &QHotkey::activated, this, &GlobalShortcuts::takeScreenshotTriggered);
    }

    m_shortcuts[id] = { hotkey, seq };
    qDebug() << "Registered global shortcut:" << id << "=" << seq.toString();
}
```

- [ ] **Step 4: Integrate in main.cpp**

Add to `qt/src/main.cpp` after creating MainWindow:
```cpp
#include "GlobalShortcuts.h"

// After MainWindow and SystemTray setup:
GlobalShortcuts *globalShortcuts = new GlobalShortcuts(&app);

if (GlobalShortcuts::isSupported()) {
    globalShortcuts->registerAll();

    QObject::connect(globalShortcuts, &GlobalShortcuts::startStopRecordingTriggered,
                     &mainWindow, &MainWindow::onStartRecording);
    QObject::connect(globalShortcuts, &GlobalShortcuts::takeScreenshotTriggered,
                     &mainWindow, &MainWindow::onTakeScreenshot);
    QObject::connect(globalShortcuts, &GlobalShortcuts::pauseResumeRecordingTriggered,
                     &mainWindow, &MainWindow::onPauseResumeRecording);

    // Re-register when settings change
    QObject::connect(&AppState::instance(), &AppState::settingsChanged,
                     globalShortcuts, &GlobalShortcuts::reloadFromSettings);
} else {
    qWarning() << "Global shortcuts are not supported on this platform."
               << "In-app shortcuts will still work.";
}
```

- [ ] **Step 5: Add in-app shortcuts to MainWindow**

Add to `qt/src/MainWindow.cpp` (these work when the app is focused):
```cpp
#include <QShortcut>

// In MainWindow constructor or setupUi():
void MainWindow::setupInAppShortcuts()
{
    auto &shortcuts = AppState::instance().settings().shortcuts;

    auto parseKey = [](const QString &s) -> QKeySequence {
        QString q = s;
        q.replace("CommandOrControl", "Ctrl");
        return QKeySequence(q);
    };

    auto *startStop = new QShortcut(
        parseKey(shortcuts.start_stop_recording), this);
    connect(startStop, &QShortcut::activated, this, &MainWindow::onStartRecording);

    auto *screenshot = new QShortcut(
        parseKey(shortcuts.take_screenshot), this);
    connect(screenshot, &QShortcut::activated, this, &MainWindow::onTakeScreenshot);

    auto *pauseResume = new QShortcut(
        parseKey(shortcuts.pause_resume_recording), this);
    connect(pauseResume, &QShortcut::activated, this, &MainWindow::onPauseResumeRecording);
}
```

- [ ] **Step 6: Add to CMakeLists.txt and verify**

Add to the `SOURCES` list:
```cmake
src/GlobalShortcuts.h
src/GlobalShortcuts.cpp
```

```bash
cd /home/rw3iss/Sites/others/screen-recorder/qt
cmake --build build --target ScreenDream 2>&1 | tail -20
```

---

## Phase 7: Editor, Settings & Packaging (Tasks 22–29)

---

## Task 22: EditorWindow Shell

**Files:**
- Create: `qt/src/OverlayItem.h`
- Create: `qt/src/OverlayItem.cpp`
- Create: `qt/src/EditorWindow.h`
- Create: `qt/src/EditorWindow.cpp`
- Modify: `qt/CMakeLists.txt`

- [ ] **Step 1: Create OverlayItem data model**

Create `qt/src/OverlayItem.h`:
```cpp
#ifndef OVERLAYITEM_H
#define OVERLAYITEM_H

#include <QString>
#include <QRectF>
#include <QColor>
#include <QFont>
#include <QImage>
#include <QPointF>
#include <QVector>

/// Represents an overlay element in the editor (text, shape, blur, image).
/// Used by the preview canvas, layer panel, and properties panel.
struct OverlayItem {
    enum Type {
        Text,
        Rectangle,
        Ellipse,
        Arrow,
        BlurRegion,
        ImageOverlay,
    };

    int id = 0;
    Type type = Rectangle;
    QString name;
    bool visible = true;
    int zOrder = 0;

    // Geometry (in image/video coordinates, not widget coordinates)
    QRectF rect;

    // Common properties
    QColor strokeColor = QColor(255, 59, 48);
    QColor fillColor = Qt::transparent;
    int strokeWidth = 2;
    qreal opacity = 1.0;

    // Text-specific
    QString text;
    QFont font = QFont("Sans", 24);
    QColor textColor = Qt::white;

    // Arrow-specific
    QPointF arrowStart;
    QPointF arrowEnd;

    // Blur-specific
    int blurRadius = 20;

    // Image overlay-specific
    QImage overlayImage;
    QString imagePath;
};

/// List of overlay items for a document.
using OverlayItemList = QVector<OverlayItem>;

#endif // OVERLAYITEM_H
```

- [ ] **Step 2: Create OverlayItem.cpp (minimal)**

Create `qt/src/OverlayItem.cpp`:
```cpp
#include "OverlayItem.h"

// OverlayItem is a POD-like struct; implementation is intentionally minimal.
// Complex serialization (save/load) can be added here later.
```

- [ ] **Step 3: Create EditorWindow header**

Create `qt/src/EditorWindow.h`:
```cpp
#ifndef EDITORWINDOW_H
#define EDITORWINDOW_H

#include <QMainWindow>
#include <QSplitter>
#include <QDockWidget>
#include <QPushButton>

#include "OverlayItem.h"

// Forward declarations
class VideoPreview;
class ToolPanel;
class TimelineWidget;
class LayerPanel;

/// Main editor window for captured images and videos.
///
/// Layout:
///   - Central: QSplitter with preview canvas
///   - Left dock: Tool panel (vertical tool buttons)
///   - Right dock: Properties panel
///   - Bottom dock: Layers panel + Timeline (for video)
///   - Bottom bar: Export, Save, Copy to Clipboard buttons
///
/// Opens when the user double-clicks a recent capture or finishes recording.
class EditorWindow : public QMainWindow {
    Q_OBJECT

public:
    /// @param mediaPath Path to the image or video file to edit.
    /// @param parent Parent widget.
    explicit EditorWindow(const QString &mediaPath, QWidget *parent = nullptr);
    ~EditorWindow() override = default;

    /// Returns true if the loaded media is a video.
    bool isVideo() const { return m_isVideo; }

private slots:
    void onExportClicked();
    void onSaveClicked();
    void onCopyToClipboard();
    void onToolChanged(int toolType);
    void onLayerSelectionChanged(int layerId);

private:
    void setupUi();
    void setupMenuBar();
    void setupDocks();
    void setupBottomBar();
    void loadMedia(const QString &path);
    bool detectIsVideo(const QString &path) const;

    QString m_mediaPath;
    bool m_isVideo = false;

    // Central area
    QSplitter *m_centralSplitter = nullptr;
    VideoPreview *m_preview = nullptr;

    // Dock widgets
    QDockWidget *m_toolDock = nullptr;
    QDockWidget *m_propertiesDock = nullptr;
    QDockWidget *m_layersDock = nullptr;
    QDockWidget *m_timelineDock = nullptr;

    // Panels
    ToolPanel *m_toolPanel = nullptr;
    LayerPanel *m_layerPanel = nullptr;
    TimelineWidget *m_timeline = nullptr;

    // Bottom bar
    QPushButton *m_exportButton = nullptr;
    QPushButton *m_saveButton = nullptr;
    QPushButton *m_copyButton = nullptr;

    // Document model
    OverlayItemList m_overlays;
    int m_nextOverlayId = 1;
};

#endif // EDITORWINDOW_H
```

- [ ] **Step 4: Create EditorWindow implementation**

Create `qt/src/EditorWindow.cpp`:
```cpp
#include "EditorWindow.h"
#include "VideoPreview.h"
#include "ToolPanel.h"
#include "TimelineWidget.h"
#include "LayerPanel.h"
#include "ExportDialog.h"
#include "RustBridge.h"

#include <QMenuBar>
#include <QStatusBar>
#include <QToolBar>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QFileInfo>
#include <QMimeDatabase>
#include <QClipboard>
#include <QApplication>
#include <QImage>
#include <QMessageBox>

EditorWindow::EditorWindow(const QString &mediaPath, QWidget *parent)
    : QMainWindow(parent)
    , m_mediaPath(mediaPath)
{
    setWindowTitle(tr("Screen Dream Editor - %1").arg(QFileInfo(mediaPath).fileName()));
    setMinimumSize(1024, 700);
    resize(1280, 800);

    m_isVideo = detectIsVideo(mediaPath);

    setupUi();
    setupMenuBar();
    setupDocks();
    setupBottomBar();
    loadMedia(mediaPath);
}

void EditorWindow::setupUi()
{
    // Central splitter with preview canvas
    m_centralSplitter = new QSplitter(Qt::Horizontal, this);
    m_preview = new VideoPreview(this);
    m_centralSplitter->addWidget(m_preview);
    setCentralWidget(m_centralSplitter);
}

void EditorWindow::setupMenuBar()
{
    auto *fileMenu = menuBar()->addMenu(tr("&File"));
    fileMenu->addAction(tr("&Save"), this, &EditorWindow::onSaveClicked,
                        QKeySequence::Save);
    fileMenu->addAction(tr("&Export..."), this, &EditorWindow::onExportClicked,
                        QKeySequence(Qt::CTRL | Qt::Key_E));
    fileMenu->addSeparator();
    fileMenu->addAction(tr("&Close"), this, &QWidget::close,
                        QKeySequence::Close);

    auto *editMenu = menuBar()->addMenu(tr("&Edit"));
    editMenu->addAction(tr("&Undo"), QKeySequence::Undo);
    editMenu->addAction(tr("&Redo"), QKeySequence::Redo);
    editMenu->addSeparator();
    editMenu->addAction(tr("Copy to &Clipboard"), this,
                        &EditorWindow::onCopyToClipboard,
                        QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_C));
}

void EditorWindow::setupDocks()
{
    // --- Left dock: Tool panel ---
    m_toolDock = new QDockWidget(tr("Tools"), this);
    m_toolDock->setAllowedAreas(Qt::LeftDockWidgetArea | Qt::RightDockWidgetArea);
    m_toolDock->setFeatures(QDockWidget::DockWidgetMovable);
    m_toolPanel = new ToolPanel(this);
    m_toolDock->setWidget(m_toolPanel);
    addDockWidget(Qt::LeftDockWidgetArea, m_toolDock);

    connect(m_toolPanel, &ToolPanel::toolChanged,
            this, &EditorWindow::onToolChanged);

    // --- Right dock: Properties panel ---
    m_propertiesDock = new QDockWidget(tr("Properties"), this);
    m_propertiesDock->setAllowedAreas(Qt::LeftDockWidgetArea | Qt::RightDockWidgetArea);
    // Properties panel content is a placeholder for now
    auto *propsPlaceholder = new QWidget(this);
    propsPlaceholder->setMinimumWidth(200);
    m_propertiesDock->setWidget(propsPlaceholder);
    addDockWidget(Qt::RightDockWidgetArea, m_propertiesDock);

    // --- Bottom dock: Layers panel ---
    m_layersDock = new QDockWidget(tr("Layers"), this);
    m_layersDock->setAllowedAreas(Qt::BottomDockWidgetArea);
    m_layerPanel = new LayerPanel(this);
    m_layersDock->setWidget(m_layerPanel);
    addDockWidget(Qt::BottomDockWidgetArea, m_layersDock);

    connect(m_layerPanel, &LayerPanel::layerSelected,
            this, &EditorWindow::onLayerSelectionChanged);

    // --- Bottom dock: Timeline (video only) ---
    if (m_isVideo) {
        m_timelineDock = new QDockWidget(tr("Timeline"), this);
        m_timelineDock->setAllowedAreas(Qt::BottomDockWidgetArea);
        m_timeline = new TimelineWidget(this);
        m_timelineDock->setWidget(m_timeline);
        addDockWidget(Qt::BottomDockWidgetArea, m_timelineDock);

        // Tabify layers and timeline in the bottom area
        tabifyDockWidget(m_layersDock, m_timelineDock);
        m_timelineDock->raise(); // show timeline tab by default for video

        connect(m_timeline, &TimelineWidget::playheadMoved,
                m_preview, &VideoPreview::seekToTime);
    }
}

void EditorWindow::setupBottomBar()
{
    auto *bottomBar = new QToolBar(tr("Actions"), this);
    bottomBar->setMovable(false);
    bottomBar->setFloatable(false);

    bottomBar->addWidget(new QWidget(this)); // spacer trick
    auto *spacer = new QWidget(this);
    spacer->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    bottomBar->addWidget(spacer);

    m_copyButton = new QPushButton(tr("Copy to Clipboard"), this);
    connect(m_copyButton, &QPushButton::clicked,
            this, &EditorWindow::onCopyToClipboard);
    bottomBar->addWidget(m_copyButton);

    m_saveButton = new QPushButton(tr("Save"), this);
    connect(m_saveButton, &QPushButton::clicked,
            this, &EditorWindow::onSaveClicked);
    bottomBar->addWidget(m_saveButton);

    m_exportButton = new QPushButton(tr("Export"), this);
    m_exportButton->setStyleSheet(
        "QPushButton { background-color: #0A84FF; color: white; "
        "padding: 6px 16px; border-radius: 4px; font-weight: bold; }");
    connect(m_exportButton, &QPushButton::clicked,
            this, &EditorWindow::onExportClicked);
    bottomBar->addWidget(m_exportButton);

    addToolBar(Qt::BottomToolBarArea, bottomBar);
}

void EditorWindow::loadMedia(const QString &path)
{
    if (m_isVideo) {
        // Load first frame via RustBridge (FFmpeg frame extraction)
        QImage firstFrame = RustBridge::instance().extractFrame(path, 0.0);
        m_preview->setBaseImage(firstFrame);

        // Set timeline duration
        double duration = RustBridge::instance().getMediaDuration(path);
        if (m_timeline) {
            m_timeline->setDuration(duration);
        }
    } else {
        // Load image directly
        QImage image(path);
        m_preview->setBaseImage(image);
    }

    // Add base layer
    OverlayItem baseLayer;
    baseLayer.id = m_nextOverlayId++;
    baseLayer.type = OverlayItem::ImageOverlay;
    baseLayer.name = QFileInfo(path).fileName();
    baseLayer.visible = true;
    baseLayer.zOrder = 0;
    m_overlays.append(baseLayer);
    m_layerPanel->setOverlays(m_overlays);
}

bool EditorWindow::detectIsVideo(const QString &path) const
{
    QMimeDatabase mimeDb;
    QString mimeType = mimeDb.mimeTypeForFile(path).name();
    return mimeType.startsWith("video/");
}

void EditorWindow::onExportClicked()
{
    ExportDialog dialog(m_mediaPath, m_isVideo, this);
    dialog.exec();
}

void EditorWindow::onSaveClicked()
{
    // For images: render overlays onto image and save
    // For videos: save project state (overlays, trims) as JSON sidecar
    statusBar()->showMessage(tr("Saved."), 3000);
}

void EditorWindow::onCopyToClipboard()
{
    QImage rendered = m_preview->renderComposite();
    if (!rendered.isNull()) {
        QApplication::clipboard()->setImage(rendered);
        statusBar()->showMessage(tr("Copied to clipboard."), 3000);
    }
}

void EditorWindow::onToolChanged(int toolType)
{
    m_preview->setActiveTool(toolType);
}

void EditorWindow::onLayerSelectionChanged(int layerId)
{
    m_preview->setSelectedOverlay(layerId);
}
```

- [ ] **Step 5: Add to CMakeLists.txt**

Add to the `SOURCES` list:
```cmake
src/OverlayItem.h
src/OverlayItem.cpp
src/EditorWindow.h
src/EditorWindow.cpp
```

- [ ] **Step 6: Verify compilation**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/qt
cmake --build build --target ScreenDream 2>&1 | tail -20
```

---

## Task 23: VideoPreview Widget

**Files:**
- Create: `qt/src/VideoPreview.h`
- Create: `qt/src/VideoPreview.cpp`
- Modify: `qt/CMakeLists.txt`

- [ ] **Step 1: Create VideoPreview header**

Create `qt/src/VideoPreview.h`:
```cpp
#ifndef VIDEOPREVIEW_H
#define VIDEOPREVIEW_H

#include <QWidget>
#include <QImage>
#include <QPointF>
#include <QRectF>

#include "OverlayItem.h"

/// Preview canvas widget for the editor.
///
/// Displays a QImage (frame or screenshot) scaled to fit with aspect ratio
/// preserved. Renders overlay items on top (text, shapes, blur regions).
/// Handles mouse interaction for selecting and moving overlays.
///
/// In v1, uses QPainter for all rendering (not QOpenGLWidget).
class VideoPreview : public QWidget {
    Q_OBJECT

public:
    explicit VideoPreview(QWidget *parent = nullptr);
    ~VideoPreview() override = default;

    /// Set the base image (the captured frame or screenshot).
    void setBaseImage(const QImage &image);

    /// Set the list of overlays to render on top of the base image.
    void setOverlays(const OverlayItemList &overlays);

    /// Set which tool is currently active (affects mouse interaction).
    void setActiveTool(int toolType);

    /// Highlight a specific overlay as selected.
    void setSelectedOverlay(int overlayId);

    /// Render the composite image (base + all overlays) for export/clipboard.
    QImage renderComposite() const;

public slots:
    /// Seek to a specific time in a video (updates the base image via FFmpeg).
    void seekToTime(double seconds);

signals:
    /// Emitted when the user clicks on an overlay to select it.
    void overlaySelected(int overlayId);

    /// Emitted when the user moves an overlay.
    void overlayMoved(int overlayId, QRectF newRect);

    /// Emitted when the user creates a new overlay via tool interaction.
    void overlayCreated(OverlayItem item);

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void resizeEvent(QResizeEvent *event) override;

private:
    /// Convert widget coordinates to image coordinates.
    QPointF widgetToImage(const QPointF &widgetPos) const;

    /// Convert image coordinates to widget coordinates.
    QPointF imageToWidget(const QPointF &imagePos) const;

    /// Get the display rect (where the image is drawn in the widget).
    QRectF displayRect() const;

    /// Find which overlay (if any) is under the given image-space point.
    int hitTestOverlay(const QPointF &imagePos) const;

    /// Render a single overlay item using QPainter.
    void renderOverlay(QPainter &painter, const OverlayItem &item,
                       const QRectF &dispRect, double scaleX, double scaleY) const;

    QImage m_baseImage;
    OverlayItemList m_overlays;
    int m_activeTool = 0; // 0 = Select
    int m_selectedOverlayId = -1;

    // Mouse interaction state
    bool m_dragging = false;
    QPointF m_dragStartImagePos;
    QRectF m_dragStartRect;
    int m_dragOverlayId = -1;

    // For tool creation (drawing new shapes)
    bool m_creating = false;
    QPointF m_createStartPos;

    // Media path for video frame extraction
    QString m_mediaPath;
};

#endif // VIDEOPREVIEW_H
```

- [ ] **Step 2: Create VideoPreview implementation**

Create `qt/src/VideoPreview.cpp`:
```cpp
#include "VideoPreview.h"
#include "RustBridge.h"

#include <QPainter>
#include <QMouseEvent>
#include <QResizeEvent>
#include <cmath>

VideoPreview::VideoPreview(QWidget *parent)
    : QWidget(parent)
{
    setMinimumSize(320, 240);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
    setMouseTracking(true);
    setFocusPolicy(Qt::ClickFocus);

    // Dark background
    setAutoFillBackground(true);
    QPalette pal = palette();
    pal.setColor(QPalette::Window, QColor(24, 24, 24));
    setPalette(pal);
}

void VideoPreview::setBaseImage(const QImage &image)
{
    m_baseImage = image;
    update();
}

void VideoPreview::setOverlays(const OverlayItemList &overlays)
{
    m_overlays = overlays;
    update();
}

void VideoPreview::setActiveTool(int toolType)
{
    m_activeTool = toolType;
    // Update cursor based on tool
    switch (toolType) {
    case 0: setCursor(Qt::ArrowCursor); break;   // Select
    case 1: setCursor(Qt::IBeamCursor); break;    // Text
    default: setCursor(Qt::CrossCursor); break;   // Shape tools
    }
}

void VideoPreview::setSelectedOverlay(int overlayId)
{
    m_selectedOverlayId = overlayId;
    update();
}

QRectF VideoPreview::displayRect() const
{
    if (m_baseImage.isNull()) return QRectF();

    double widgetW = width();
    double widgetH = height();
    double imageW = m_baseImage.width();
    double imageH = m_baseImage.height();

    double scale = qMin(widgetW / imageW, widgetH / imageH);
    double dispW = imageW * scale;
    double dispH = imageH * scale;
    double x = (widgetW - dispW) / 2.0;
    double y = (widgetH - dispH) / 2.0;

    return QRectF(x, y, dispW, dispH);
}

QPointF VideoPreview::widgetToImage(const QPointF &widgetPos) const
{
    QRectF dr = displayRect();
    if (dr.isEmpty() || m_baseImage.isNull()) return QPointF();

    double scaleX = m_baseImage.width() / dr.width();
    double scaleY = m_baseImage.height() / dr.height();

    return QPointF(
        (widgetPos.x() - dr.x()) * scaleX,
        (widgetPos.y() - dr.y()) * scaleY);
}

QPointF VideoPreview::imageToWidget(const QPointF &imagePos) const
{
    QRectF dr = displayRect();
    if (dr.isEmpty() || m_baseImage.isNull()) return QPointF();

    double scaleX = dr.width() / m_baseImage.width();
    double scaleY = dr.height() / m_baseImage.height();

    return QPointF(
        imagePos.x() * scaleX + dr.x(),
        imagePos.y() * scaleY + dr.y());
}

void VideoPreview::paintEvent(QPaintEvent * /*event*/)
{
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setRenderHint(QPainter::SmoothPixmapTransform);

    if (m_baseImage.isNull()) {
        painter.setPen(QColor(100, 100, 100));
        painter.drawText(rect(), Qt::AlignCenter, tr("No media loaded"));
        return;
    }

    QRectF dr = displayRect();

    // Draw base image scaled to fit
    painter.drawImage(dr, m_baseImage);

    // Draw overlays
    double scaleX = dr.width() / m_baseImage.width();
    double scaleY = dr.height() / m_baseImage.height();

    for (const auto &overlay : m_overlays) {
        if (!overlay.visible) continue;
        renderOverlay(painter, overlay, dr, scaleX, scaleY);
    }

    // Draw selection handles on selected overlay
    if (m_selectedOverlayId >= 0) {
        for (const auto &overlay : m_overlays) {
            if (overlay.id != m_selectedOverlayId) continue;

            QRectF itemWidget(
                overlay.rect.x() * scaleX + dr.x(),
                overlay.rect.y() * scaleY + dr.y(),
                overlay.rect.width() * scaleX,
                overlay.rect.height() * scaleY);

            painter.setPen(QPen(QColor(0, 120, 255), 1.5, Qt::DashLine));
            painter.setBrush(Qt::NoBrush);
            painter.drawRect(itemWidget);

            // Corner handles
            const double hs = 5.0;
            painter.setBrush(QColor(0, 120, 255));
            painter.setPen(Qt::NoPen);
            QPointF corners[] = {
                itemWidget.topLeft(), itemWidget.topRight(),
                itemWidget.bottomLeft(), itemWidget.bottomRight()
            };
            for (const auto &c : corners) {
                painter.drawRect(QRectF(c.x() - hs, c.y() - hs, hs * 2, hs * 2));
            }
            break;
        }
    }

    // Draw creation preview (when dragging to create a new shape)
    if (m_creating) {
        QPointF startW = imageToWidget(m_createStartPos);
        QPointF endW = mapFromGlobal(QCursor::pos());
        QRectF createRect = QRectF(startW, endW).normalized();

        painter.setPen(QPen(QColor(0, 120, 255), 1.5, Qt::DashLine));
        painter.setBrush(QColor(0, 120, 255, 30));
        painter.drawRect(createRect);
    }
}

void VideoPreview::renderOverlay(QPainter &painter, const OverlayItem &item,
                                  const QRectF &dispRect,
                                  double scaleX, double scaleY) const
{
    QRectF r(
        item.rect.x() * scaleX + dispRect.x(),
        item.rect.y() * scaleY + dispRect.y(),
        item.rect.width() * scaleX,
        item.rect.height() * scaleY);

    painter.save();
    painter.setOpacity(item.opacity);

    switch (item.type) {
    case OverlayItem::Text: {
        QFont scaledFont = item.font;
        scaledFont.setPointSizeF(item.font.pointSizeF() * scaleY);
        painter.setFont(scaledFont);
        painter.setPen(item.textColor);
        painter.drawText(r, Qt::AlignLeft | Qt::AlignTop | Qt::TextWordWrap, item.text);
        break;
    }
    case OverlayItem::Rectangle:
        painter.setPen(QPen(item.strokeColor, item.strokeWidth));
        painter.setBrush(item.fillColor);
        painter.drawRect(r);
        break;

    case OverlayItem::Ellipse:
        painter.setPen(QPen(item.strokeColor, item.strokeWidth));
        painter.setBrush(item.fillColor);
        painter.drawEllipse(r);
        break;

    case OverlayItem::Arrow: {
        QPointF start(item.arrowStart.x() * scaleX + dispRect.x(),
                      item.arrowStart.y() * scaleY + dispRect.y());
        QPointF end(item.arrowEnd.x() * scaleX + dispRect.x(),
                    item.arrowEnd.y() * scaleY + dispRect.y());

        painter.setPen(QPen(item.strokeColor, item.strokeWidth));
        painter.drawLine(start, end);

        // Draw arrowhead
        double angle = std::atan2(end.y() - start.y(), end.x() - start.x());
        double headLen = 12.0;
        QPointF p1(end.x() - headLen * std::cos(angle - M_PI / 6),
                   end.y() - headLen * std::sin(angle - M_PI / 6));
        QPointF p2(end.x() - headLen * std::cos(angle + M_PI / 6),
                   end.y() - headLen * std::sin(angle + M_PI / 6));
        QPolygonF arrowHead;
        arrowHead << end << p1 << p2;
        painter.setBrush(item.strokeColor);
        painter.drawPolygon(arrowHead);
        break;
    }
    case OverlayItem::BlurRegion:
        // In v1, draw a pixelated/mosaic representation
        painter.setPen(QPen(QColor(200, 200, 200, 100), 1, Qt::DashLine));
        painter.setBrush(QColor(128, 128, 128, 80));
        painter.drawRect(r);
        painter.setPen(QColor(200, 200, 200, 150));
        painter.drawText(r, Qt::AlignCenter, tr("Blur"));
        break;

    case OverlayItem::ImageOverlay:
        if (!item.overlayImage.isNull()) {
            painter.drawImage(r, item.overlayImage);
        }
        break;
    }

    painter.restore();
}

int VideoPreview::hitTestOverlay(const QPointF &imagePos) const
{
    // Iterate in reverse z-order (top-most first)
    for (int i = m_overlays.size() - 1; i >= 0; --i) {
        const auto &overlay = m_overlays[i];
        if (!overlay.visible) continue;
        if (overlay.rect.contains(imagePos)) {
            return overlay.id;
        }
    }
    return -1;
}

void VideoPreview::mousePressEvent(QMouseEvent *event)
{
    if (event->button() != Qt::LeftButton) return;

    QPointF imagePos = widgetToImage(event->position());

    if (m_activeTool == 0) {
        // Select tool: try to select an overlay
        int hitId = hitTestOverlay(imagePos);
        m_selectedOverlayId = hitId;
        emit overlaySelected(hitId);

        if (hitId >= 0) {
            // Start dragging
            m_dragging = true;
            m_dragOverlayId = hitId;
            m_dragStartImagePos = imagePos;
            for (const auto &o : m_overlays) {
                if (o.id == hitId) {
                    m_dragStartRect = o.rect;
                    break;
                }
            }
        }
    } else {
        // Shape/text creation tools: start creating
        m_creating = true;
        m_createStartPos = imagePos;
    }

    update();
}

void VideoPreview::mouseMoveEvent(QMouseEvent *event)
{
    if (m_dragging) {
        QPointF imagePos = widgetToImage(event->position());
        QPointF delta = imagePos - m_dragStartImagePos;

        for (auto &overlay : m_overlays) {
            if (overlay.id == m_dragOverlayId) {
                overlay.rect = m_dragStartRect.translated(delta);
                emit overlayMoved(overlay.id, overlay.rect);
                break;
            }
        }
        update();
    } else if (m_creating) {
        update(); // repaint to show creation preview
    }
}

void VideoPreview::mouseReleaseEvent(QMouseEvent *event)
{
    if (event->button() != Qt::LeftButton) return;

    if (m_dragging) {
        m_dragging = false;
        m_dragOverlayId = -1;
    }

    if (m_creating) {
        m_creating = false;
        QPointF imageEnd = widgetToImage(event->position());
        QRectF newRect = QRectF(m_createStartPos, imageEnd).normalized();

        // Minimum size check
        if (newRect.width() > 5 && newRect.height() > 5) {
            OverlayItem item;
            item.rect = newRect;

            // Map tool type to overlay type
            switch (m_activeTool) {
            case 1: item.type = OverlayItem::Text; item.text = tr("Text"); break;
            case 2: item.type = OverlayItem::Rectangle; break;
            case 3: item.type = OverlayItem::Ellipse; break;
            case 4: item.type = OverlayItem::Arrow;
                    item.arrowStart = m_createStartPos;
                    item.arrowEnd = imageEnd;
                    break;
            case 5: item.type = OverlayItem::BlurRegion; break;
            default: item.type = OverlayItem::Rectangle; break;
            }

            emit overlayCreated(item);
        }
        update();
    }
}

void VideoPreview::resizeEvent(QResizeEvent * /*event*/)
{
    update();
}

void VideoPreview::seekToTime(double seconds)
{
    if (m_mediaPath.isEmpty()) return;

    QImage frame = RustBridge::instance().extractFrame(m_mediaPath, seconds);
    if (!frame.isNull()) {
        m_baseImage = frame;
        update();
    }
}

QImage VideoPreview::renderComposite() const
{
    if (m_baseImage.isNull()) return QImage();

    QImage result = m_baseImage.copy();
    QPainter painter(&result);
    painter.setRenderHint(QPainter::Antialiasing);

    for (const auto &overlay : m_overlays) {
        if (!overlay.visible) continue;
        renderOverlay(painter, overlay, QRectF(0, 0, result.width(), result.height()),
                      1.0, 1.0);
    }

    return result;
}
```

- [ ] **Step 3: Add to CMakeLists.txt and verify**

Add to the `SOURCES` list:
```cmake
src/VideoPreview.h
src/VideoPreview.cpp
```

```bash
cd /home/rw3iss/Sites/others/screen-recorder/qt
cmake --build build --target ScreenDream 2>&1 | tail -20
```

---

## Task 24: Tool Panel

**Files:**
- Create: `qt/src/ToolPanel.h`
- Create: `qt/src/ToolPanel.cpp`
- Modify: `qt/CMakeLists.txt`

- [ ] **Step 1: Create ToolPanel header**

Create `qt/src/ToolPanel.h`:
```cpp
#ifndef TOOLPANEL_H
#define TOOLPANEL_H

#include <QWidget>
#include <QButtonGroup>
#include <QVBoxLayout>
#include <QPushButton>

/// Vertical panel with tool buttons for the editor.
///
/// Each button activates a mode on the preview canvas:
///   0 = Select, 1 = Text, 2 = Rectangle, 3 = Ellipse,
///   4 = Arrow, 5 = Blur, 6 = Image, 7 = Crop, 8 = Scale
class ToolPanel : public QWidget {
    Q_OBJECT

public:
    /// Tool type constants (match array indices).
    enum Tool {
        Select  = 0,
        Text    = 1,
        Rect    = 2,
        Ellipse = 3,
        Arrow   = 4,
        Blur    = 5,
        Image   = 6,
        Crop    = 7,
        Scale   = 8,
    };
    Q_ENUM(Tool)

    explicit ToolPanel(QWidget *parent = nullptr);
    ~ToolPanel() override = default;

    /// Returns the currently selected tool.
    int currentTool() const;

signals:
    /// Emitted when the user selects a different tool.
    void toolChanged(int toolType);

private:
    void addToolButton(int toolId, const QString &iconName,
                       const QString &tooltip);

    QVBoxLayout *m_layout = nullptr;
    QButtonGroup *m_buttonGroup = nullptr;
};

#endif // TOOLPANEL_H
```

- [ ] **Step 2: Create ToolPanel implementation**

Create `qt/src/ToolPanel.cpp`:
```cpp
#include "ToolPanel.h"

#include <QIcon>
#include <QToolButton>

ToolPanel::ToolPanel(QWidget *parent)
    : QWidget(parent)
{
    setFixedWidth(48);

    m_layout = new QVBoxLayout(this);
    m_layout->setContentsMargins(4, 8, 4, 8);
    m_layout->setSpacing(4);

    m_buttonGroup = new QButtonGroup(this);
    m_buttonGroup->setExclusive(true);

    // Define tools: (id, icon resource, tooltip)
    struct ToolDef {
        int id;
        QString icon;
        QString tooltip;
        QString fallbackText; // unicode fallback if icon missing
    };
    const ToolDef tools[] = {
        { Select,  "tool-select",  tr("Select (V)"),           QString::fromUtf8("\xE2\x86\x96") },  // arrow
        { Text,    "tool-text",    tr("Text (T)"),             "T"  },
        { Rect,    "tool-rect",    tr("Rectangle (R)"),        QString::fromUtf8("\xE2\x96\xAD") },
        { Ellipse, "tool-ellipse", tr("Ellipse (E)"),          "O"  },
        { Arrow,   "tool-arrow",   tr("Arrow (A)"),            QString::fromUtf8("\xE2\x86\x97") },
        { Blur,    "tool-blur",    tr("Blur Region (B)"),      QString::fromUtf8("\xE2\x96\xA3") },
        { Image,   "tool-image",   tr("Image Overlay (I)"),    QString::fromUtf8("\xF0\x9F\x96\xBC") },
        { Crop,    "tool-crop",    tr("Crop (C)"),             QString::fromUtf8("\xE2\x8C\x98") },
        { Scale,   "tool-scale",   tr("Scale (S)"),            QString::fromUtf8("\xE2\x87\xB2") },
    };

    for (const auto &tool : tools) {
        auto *btn = new QToolButton(this);
        btn->setCheckable(true);
        btn->setToolTip(tool.tooltip);
        btn->setFixedSize(40, 40);
        btn->setIconSize(QSize(24, 24));

        // Try to load icon from resources; fall back to text
        QIcon icon(QString(":/icons/%1.png").arg(tool.icon));
        if (icon.isNull() || icon.availableSizes().isEmpty()) {
            btn->setText(tool.fallbackText);
            btn->setStyleSheet(
                "QToolButton { font-size: 18px; }"
                "QToolButton:checked { background-color: #0A84FF; color: white; border-radius: 4px; }");
        } else {
            btn->setIcon(icon);
            btn->setStyleSheet(
                "QToolButton:checked { background-color: #0A84FF; border-radius: 4px; }");
        }

        m_buttonGroup->addButton(btn, tool.id);
        m_layout->addWidget(btn);
    }

    m_layout->addStretch();

    // Select tool is active by default
    if (auto *firstBtn = m_buttonGroup->button(Select)) {
        firstBtn->setChecked(true);
    }

    connect(m_buttonGroup, &QButtonGroup::idClicked,
            this, &ToolPanel::toolChanged);
}

int ToolPanel::currentTool() const
{
    return m_buttonGroup->checkedId();
}
```

- [ ] **Step 3: Add to CMakeLists.txt and verify**

Add to the `SOURCES` list:
```cmake
src/ToolPanel.h
src/ToolPanel.cpp
```

```bash
cd /home/rw3iss/Sites/others/screen-recorder/qt
cmake --build build --target ScreenDream 2>&1 | tail -20
```

---

## Task 25: Timeline Widget (video only)

**Files:**
- Create: `qt/src/TimelineWidget.h`
- Create: `qt/src/TimelineWidget.cpp`
- Modify: `qt/CMakeLists.txt`

- [ ] **Step 1: Create TimelineWidget header**

Create `qt/src/TimelineWidget.h`:
```cpp
#ifndef TIMELINEWIDGET_H
#define TIMELINEWIDGET_H

#include <QWidget>
#include <QScrollBar>

/// Custom-painted timeline widget for video editing.
///
/// Features:
///   - Time ruler with tick marks and time labels
///   - Video track bar (colored rectangle)
///   - Audio waveform track (simplified colored bar for v1)
///   - Trim handles at start/end (draggable)
///   - Playhead (vertical line, draggable)
///   - Zoom in/out on time axis
class TimelineWidget : public QWidget {
    Q_OBJECT

public:
    explicit TimelineWidget(QWidget *parent = nullptr);
    ~TimelineWidget() override = default;

    /// Set the total duration of the video in seconds.
    void setDuration(double seconds);

    /// Get current playhead position in seconds.
    double playheadPosition() const;

    /// Get trim range.
    double trimStart() const { return m_trimStart; }
    double trimEnd() const { return m_trimEnd; }

public slots:
    /// Set playhead position programmatically.
    void setPlayheadPosition(double seconds);

    /// Zoom in on the time axis.
    void zoomIn();

    /// Zoom out on the time axis.
    void zoomOut();

signals:
    /// Emitted when the playhead is moved by the user.
    void playheadMoved(double seconds);

    /// Emitted when trim handles are changed by the user.
    void trimChanged(double startSeconds, double endSeconds);

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void wheelEvent(QWheelEvent *event) override;

private:
    /// Convert seconds to pixel x-coordinate in the timeline.
    double secondsToX(double seconds) const;

    /// Convert pixel x-coordinate to seconds.
    double xToSeconds(double x) const;

    /// Draw the time ruler (top row with ticks and labels).
    void drawRuler(QPainter &painter, const QRect &rulerRect) const;

    /// Draw the video track bar.
    void drawVideoTrack(QPainter &painter, const QRect &trackRect) const;

    /// Draw the audio waveform track (simplified).
    void drawAudioTrack(QPainter &painter, const QRect &trackRect) const;

    /// Draw the playhead (vertical line).
    void drawPlayhead(QPainter &painter) const;

    /// Draw trim handles.
    void drawTrimHandles(QPainter &painter) const;

    enum DragTarget { None, Playhead, TrimStart, TrimEnd };
    DragTarget hitTest(const QPoint &pos) const;

    double m_duration = 0.0;        // total duration in seconds
    double m_playhead = 0.0;        // playhead position in seconds
    double m_trimStart = 0.0;       // trim start in seconds
    double m_trimEnd = 0.0;         // trim end in seconds (0 = full duration)

    double m_pixelsPerSecond = 40.0; // zoom level
    double m_scrollOffset = 0.0;     // horizontal scroll offset in pixels

    DragTarget m_dragTarget = None;

    // Layout constants
    static constexpr int kRulerHeight = 28;
    static constexpr int kTrackHeight = 32;
    static constexpr int kTrackGap = 4;
    static constexpr int kLeftMargin = 8;
    static constexpr int kTrimHandleWidth = 8;
};

#endif // TIMELINEWIDGET_H
```

- [ ] **Step 2: Create TimelineWidget implementation**

Create `qt/src/TimelineWidget.cpp`:
```cpp
#include "TimelineWidget.h"

#include <QPainter>
#include <QMouseEvent>
#include <QWheelEvent>
#include <cmath>

TimelineWidget::TimelineWidget(QWidget *parent)
    : QWidget(parent)
{
    setMinimumHeight(kRulerHeight + kTrackHeight * 2 + kTrackGap * 3 + 8);
    setFixedHeight(120);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    setMouseTracking(true);

    setAutoFillBackground(true);
    QPalette pal = palette();
    pal.setColor(QPalette::Window, QColor(32, 32, 32));
    setPalette(pal);
}

void TimelineWidget::setDuration(double seconds)
{
    m_duration = seconds;
    m_trimEnd = seconds;
    m_playhead = 0.0;
    update();
}

double TimelineWidget::playheadPosition() const
{
    return m_playhead;
}

void TimelineWidget::setPlayheadPosition(double seconds)
{
    m_playhead = qBound(0.0, seconds, m_duration);
    update();
}

void TimelineWidget::zoomIn()
{
    m_pixelsPerSecond = qMin(m_pixelsPerSecond * 1.5, 500.0);
    update();
}

void TimelineWidget::zoomOut()
{
    m_pixelsPerSecond = qMax(m_pixelsPerSecond / 1.5, 5.0);
    update();
}

double TimelineWidget::secondsToX(double seconds) const
{
    return kLeftMargin + seconds * m_pixelsPerSecond - m_scrollOffset;
}

double TimelineWidget::xToSeconds(double x) const
{
    return (x - kLeftMargin + m_scrollOffset) / m_pixelsPerSecond;
}

void TimelineWidget::paintEvent(QPaintEvent * /*event*/)
{
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);

    int y = 0;

    // Ruler
    QRect rulerRect(0, y, width(), kRulerHeight);
    drawRuler(painter, rulerRect);
    y += kRulerHeight + kTrackGap;

    // Video track
    QRect videoRect(kLeftMargin, y, width() - kLeftMargin * 2, kTrackHeight);
    drawVideoTrack(painter, videoRect);
    y += kTrackHeight + kTrackGap;

    // Audio track
    QRect audioRect(kLeftMargin, y, width() - kLeftMargin * 2, kTrackHeight);
    drawAudioTrack(painter, audioRect);

    // Trim handles (drawn on top)
    drawTrimHandles(painter);

    // Playhead (drawn on top of everything)
    drawPlayhead(painter);
}

void TimelineWidget::drawRuler(QPainter &painter, const QRect &rulerRect) const
{
    painter.save();
    painter.fillRect(rulerRect, QColor(40, 40, 40));

    painter.setPen(QColor(120, 120, 120));
    painter.setFont(QFont("Sans", 9));

    // Determine tick interval based on zoom level
    double tickIntervalSec = 1.0;
    if (m_pixelsPerSecond < 15) tickIntervalSec = 10.0;
    else if (m_pixelsPerSecond < 30) tickIntervalSec = 5.0;
    else if (m_pixelsPerSecond < 80) tickIntervalSec = 2.0;
    else if (m_pixelsPerSecond > 200) tickIntervalSec = 0.5;

    double startSec = qMax(0.0, xToSeconds(0));
    double endSec = qMin(m_duration, xToSeconds(width()));

    for (double t = std::floor(startSec / tickIntervalSec) * tickIntervalSec;
         t <= endSec; t += tickIntervalSec)
    {
        if (t < 0) continue;
        double x = secondsToX(t);

        // Major tick
        painter.drawLine(QPointF(x, rulerRect.bottom() - 12),
                         QPointF(x, rulerRect.bottom()));

        // Time label
        int mins = static_cast<int>(t) / 60;
        int secs = static_cast<int>(t) % 60;
        int ms = static_cast<int>((t - std::floor(t)) * 10);
        QString label;
        if (tickIntervalSec >= 1.0)
            label = QString("%1:%2").arg(mins).arg(secs, 2, 10, QChar('0'));
        else
            label = QString("%1:%2.%3").arg(mins).arg(secs, 2, 10, QChar('0')).arg(ms);

        painter.drawText(QPointF(x + 3, rulerRect.bottom() - 14), label);

        // Minor ticks (quarter intervals)
        double minorInterval = tickIntervalSec / 4.0;
        for (int i = 1; i < 4; ++i) {
            double mt = t + i * minorInterval;
            if (mt > m_duration) break;
            double mx = secondsToX(mt);
            painter.drawLine(QPointF(mx, rulerRect.bottom() - 5),
                             QPointF(mx, rulerRect.bottom()));
        }
    }

    painter.restore();
}

void TimelineWidget::drawVideoTrack(QPainter &painter, const QRect &trackRect) const
{
    painter.save();

    // Track background
    painter.fillRect(trackRect, QColor(50, 50, 50));

    // Video clip bar (from trim start to trim end)
    double clipStartX = secondsToX(m_trimStart);
    double clipEndX = secondsToX(m_trimEnd > 0 ? m_trimEnd : m_duration);

    QRectF clipRect(clipStartX, trackRect.y(),
                    clipEndX - clipStartX, trackRect.height());
    painter.fillRect(clipRect, QColor(0, 120, 255, 180));

    // Label
    painter.setPen(Qt::white);
    painter.setFont(QFont("Sans", 10));
    painter.drawText(clipRect.adjusted(6, 0, 0, 0),
                     Qt::AlignVCenter | Qt::AlignLeft, tr("Video"));

    painter.restore();
}

void TimelineWidget::drawAudioTrack(QPainter &painter, const QRect &trackRect) const
{
    painter.save();

    // Track background
    painter.fillRect(trackRect, QColor(50, 50, 50));

    // Audio clip bar (simplified — solid colored bar for v1)
    double clipStartX = secondsToX(m_trimStart);
    double clipEndX = secondsToX(m_trimEnd > 0 ? m_trimEnd : m_duration);

    QRectF clipRect(clipStartX, trackRect.y(),
                    clipEndX - clipStartX, trackRect.height());
    painter.fillRect(clipRect, QColor(52, 199, 89, 150));

    painter.setPen(Qt::white);
    painter.setFont(QFont("Sans", 10));
    painter.drawText(clipRect.adjusted(6, 0, 0, 0),
                     Qt::AlignVCenter | Qt::AlignLeft, tr("Audio"));

    painter.restore();
}

void TimelineWidget::drawPlayhead(QPainter &painter) const
{
    double x = secondsToX(m_playhead);

    painter.save();
    painter.setPen(QPen(QColor(255, 59, 48), 2));
    painter.drawLine(QPointF(x, 0), QPointF(x, height()));

    // Playhead handle (triangle at top)
    QPolygonF triangle;
    triangle << QPointF(x - 6, 0) << QPointF(x + 6, 0) << QPointF(x, 10);
    painter.setBrush(QColor(255, 59, 48));
    painter.setPen(Qt::NoPen);
    painter.drawPolygon(triangle);

    painter.restore();
}

void TimelineWidget::drawTrimHandles(QPainter &painter) const
{
    painter.save();

    double startX = secondsToX(m_trimStart);
    double endX = secondsToX(m_trimEnd > 0 ? m_trimEnd : m_duration);

    int trackTop = kRulerHeight + kTrackGap;
    int trackBottom = height();

    // Start trim handle
    QRectF startHandle(startX - kTrimHandleWidth, trackTop,
                       kTrimHandleWidth, trackBottom - trackTop);
    painter.fillRect(startHandle, QColor(255, 204, 0, 200));

    // End trim handle
    QRectF endHandle(endX, trackTop,
                     kTrimHandleWidth, trackBottom - trackTop);
    painter.fillRect(endHandle, QColor(255, 204, 0, 200));

    // Dimmed regions outside trim range
    painter.fillRect(QRectF(0, trackTop, startX, trackBottom - trackTop),
                     QColor(0, 0, 0, 120));
    painter.fillRect(QRectF(endX, trackTop, width() - endX, trackBottom - trackTop),
                     QColor(0, 0, 0, 120));

    painter.restore();
}

TimelineWidget::DragTarget TimelineWidget::hitTest(const QPoint &pos) const
{
    double startX = secondsToX(m_trimStart);
    double endX = secondsToX(m_trimEnd > 0 ? m_trimEnd : m_duration);
    double playheadX = secondsToX(m_playhead);

    // Playhead (narrow hit area)
    if (std::abs(pos.x() - playheadX) < 6 && pos.y() < kRulerHeight + 10) {
        return Playhead;
    }

    // Trim handles
    if (std::abs(pos.x() - startX) < kTrimHandleWidth + 4 && pos.y() > kRulerHeight) {
        return TrimStart;
    }
    if (std::abs(pos.x() - endX) < kTrimHandleWidth + 4 && pos.y() > kRulerHeight) {
        return TrimEnd;
    }

    // Click on ruler area = move playhead
    if (pos.y() <= kRulerHeight) {
        return Playhead;
    }

    return None;
}

void TimelineWidget::mousePressEvent(QMouseEvent *event)
{
    if (event->button() != Qt::LeftButton) return;

    m_dragTarget = hitTest(event->pos());

    if (m_dragTarget == Playhead) {
        double sec = qBound(0.0, xToSeconds(event->pos().x()), m_duration);
        m_playhead = sec;
        emit playheadMoved(sec);
        update();
    }
}

void TimelineWidget::mouseMoveEvent(QMouseEvent *event)
{
    if (m_dragTarget == None) {
        // Update cursor based on hover
        DragTarget hover = hitTest(event->pos());
        switch (hover) {
        case Playhead: setCursor(Qt::SizeHorCursor); break;
        case TrimStart:
        case TrimEnd: setCursor(Qt::SplitHCursor); break;
        default: setCursor(Qt::ArrowCursor); break;
        }
        return;
    }

    double sec = qBound(0.0, xToSeconds(event->pos().x()), m_duration);

    switch (m_dragTarget) {
    case Playhead:
        m_playhead = sec;
        emit playheadMoved(sec);
        break;
    case TrimStart:
        m_trimStart = qMin(sec, m_trimEnd - 0.1);
        emit trimChanged(m_trimStart, m_trimEnd);
        break;
    case TrimEnd:
        m_trimEnd = qMax(sec, m_trimStart + 0.1);
        emit trimChanged(m_trimStart, m_trimEnd);
        break;
    default:
        break;
    }

    update();
}

void TimelineWidget::mouseReleaseEvent(QMouseEvent * /*event*/)
{
    m_dragTarget = None;
}

void TimelineWidget::wheelEvent(QWheelEvent *event)
{
    if (event->angleDelta().y() > 0) {
        zoomIn();
    } else {
        zoomOut();
    }
}
```

- [ ] **Step 3: Add to CMakeLists.txt and verify**

Add to the `SOURCES` list:
```cmake
src/TimelineWidget.h
src/TimelineWidget.cpp
```

```bash
cd /home/rw3iss/Sites/others/screen-recorder/qt
cmake --build build --target ScreenDream 2>&1 | tail -20
```

---

## Task 26: Layer Panel

**Files:**
- Create: `qt/src/LayerPanel.h`
- Create: `qt/src/LayerPanel.cpp`
- Modify: `qt/CMakeLists.txt`

- [ ] **Step 1: Create LayerPanel header**

Create `qt/src/LayerPanel.h`:
```cpp
#ifndef LAYERPANEL_H
#define LAYERPANEL_H

#include <QWidget>
#include <QListWidget>
#include <QPushButton>
#include <QVBoxLayout>

#include "OverlayItem.h"

/// Panel for managing overlay layers in the editor.
///
/// Shows a list of layers with:
///   - Visibility toggle (eye icon)
///   - Layer name (editable)
///   - Layer type icon
///   - Drag to reorder
///
/// Add/Delete buttons at the bottom.
class LayerPanel : public QWidget {
    Q_OBJECT

public:
    explicit LayerPanel(QWidget *parent = nullptr);
    ~LayerPanel() override = default;

    /// Set the overlays to display in the layer list.
    void setOverlays(const OverlayItemList &overlays);

    /// Returns the currently selected layer ID, or -1 if none.
    int selectedLayerId() const;

signals:
    /// Emitted when the user selects a layer.
    void layerSelected(int layerId);

    /// Emitted when the user toggles layer visibility.
    void layerVisibilityChanged(int layerId, bool visible);

    /// Emitted when layers are reordered by drag-and-drop.
    void layersReordered(QVector<int> newOrder);

    /// Emitted when the user clicks the delete button.
    void layerDeleteRequested(int layerId);

    /// Emitted when the user clicks the add button.
    void layerAddRequested();

private slots:
    void onItemChanged(QListWidgetItem *item);
    void onSelectionChanged();
    void onAddClicked();
    void onDeleteClicked();

private:
    void rebuildList();

    QListWidget *m_list = nullptr;
    QPushButton *m_addButton = nullptr;
    QPushButton *m_deleteButton = nullptr;

    OverlayItemList m_overlays;
};

#endif // LAYERPANEL_H
```

- [ ] **Step 2: Create LayerPanel implementation**

Create `qt/src/LayerPanel.cpp`:
```cpp
#include "LayerPanel.h"

#include <QHBoxLayout>

LayerPanel::LayerPanel(QWidget *parent)
    : QWidget(parent)
{
    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(4, 4, 4, 4);

    m_list = new QListWidget(this);
    m_list->setDragDropMode(QAbstractItemView::InternalMove);
    m_list->setDefaultDropAction(Qt::MoveAction);
    m_list->setSelectionMode(QAbstractItemView::SingleSelection);
    layout->addWidget(m_list);

    // Buttons row
    auto *buttonLayout = new QHBoxLayout();
    m_addButton = new QPushButton(tr("+"), this);
    m_addButton->setFixedSize(28, 28);
    m_addButton->setToolTip(tr("Add Layer"));
    m_deleteButton = new QPushButton(tr("-"), this);
    m_deleteButton->setFixedSize(28, 28);
    m_deleteButton->setToolTip(tr("Delete Layer"));
    m_deleteButton->setEnabled(false);

    buttonLayout->addWidget(m_addButton);
    buttonLayout->addWidget(m_deleteButton);
    buttonLayout->addStretch();
    layout->addLayout(buttonLayout);

    connect(m_list, &QListWidget::itemChanged,
            this, &LayerPanel::onItemChanged);
    connect(m_list, &QListWidget::itemSelectionChanged,
            this, &LayerPanel::onSelectionChanged);
    connect(m_addButton, &QPushButton::clicked,
            this, &LayerPanel::onAddClicked);
    connect(m_deleteButton, &QPushButton::clicked,
            this, &LayerPanel::onDeleteClicked);
}

void LayerPanel::setOverlays(const OverlayItemList &overlays)
{
    m_overlays = overlays;
    rebuildList();
}

void LayerPanel::rebuildList()
{
    m_list->blockSignals(true);
    m_list->clear();

    // Add in reverse order (top layer = first in list)
    for (int i = m_overlays.size() - 1; i >= 0; --i) {
        const auto &overlay = m_overlays[i];

        auto *item = new QListWidgetItem(m_list);
        item->setData(Qt::UserRole, overlay.id);

        // Checkable for visibility toggle
        item->setFlags(item->flags() | Qt::ItemIsUserCheckable | Qt::ItemIsEditable);
        item->setCheckState(overlay.visible ? Qt::Checked : Qt::Unchecked);

        // Type icon prefix
        QString typePrefix;
        switch (overlay.type) {
        case OverlayItem::Text:         typePrefix = "[T] "; break;
        case OverlayItem::Rectangle:    typePrefix = "[R] "; break;
        case OverlayItem::Ellipse:      typePrefix = "[E] "; break;
        case OverlayItem::Arrow:        typePrefix = "[A] "; break;
        case OverlayItem::BlurRegion:   typePrefix = "[B] "; break;
        case OverlayItem::ImageOverlay: typePrefix = "[I] "; break;
        }

        item->setText(typePrefix + (overlay.name.isEmpty()
            ? QString("Layer %1").arg(overlay.id)
            : overlay.name));
    }

    m_list->blockSignals(false);
}

int LayerPanel::selectedLayerId() const
{
    auto *item = m_list->currentItem();
    if (!item) return -1;
    return item->data(Qt::UserRole).toInt();
}

void LayerPanel::onItemChanged(QListWidgetItem *item)
{
    int layerId = item->data(Qt::UserRole).toInt();
    bool visible = (item->checkState() == Qt::Checked);
    emit layerVisibilityChanged(layerId, visible);
}

void LayerPanel::onSelectionChanged()
{
    int layerId = selectedLayerId();
    m_deleteButton->setEnabled(layerId >= 0);
    emit layerSelected(layerId);
}

void LayerPanel::onAddClicked()
{
    emit layerAddRequested();
}

void LayerPanel::onDeleteClicked()
{
    int layerId = selectedLayerId();
    if (layerId >= 0) {
        emit layerDeleteRequested(layerId);
    }
}
```

- [ ] **Step 3: Add to CMakeLists.txt and verify**

Add to the `SOURCES` list:
```cmake
src/LayerPanel.h
src/LayerPanel.cpp
```

```bash
cd /home/rw3iss/Sites/others/screen-recorder/qt
cmake --build build --target ScreenDream 2>&1 | tail -20
```

---

## Task 27: Export Flow

**Files:**
- Create: `qt/src/ExportDialog.h`
- Create: `qt/src/ExportDialog.cpp`
- Modify: `qt/CMakeLists.txt`

- [ ] **Step 1: Create ExportDialog header**

Create `qt/src/ExportDialog.h`:
```cpp
#ifndef EXPORTDIALOG_H
#define EXPORTDIALOG_H

#include <QDialog>
#include <QComboBox>
#include <QSlider>
#include <QSpinBox>
#include <QLineEdit>
#include <QPushButton>
#include <QProgressBar>
#include <QLabel>
#include <QTimer>

/// Export dialog for saving edited media to a final output file.
///
/// Shows format, quality, resolution, and output path options.
/// Runs FFmpeg via RustBridge with progress parsing.
class ExportDialog : public QDialog {
    Q_OBJECT

public:
    /// @param inputPath Path to the source media file.
    /// @param isVideo True if exporting a video, false for an image.
    /// @param parent Parent widget.
    explicit ExportDialog(const QString &inputPath, bool isVideo,
                          QWidget *parent = nullptr);
    ~ExportDialog() override = default;

private slots:
    void onBrowseOutput();
    void onExport();
    void onCancel();
    void onExportProgress(int percent, const QString &eta);
    void onExportComplete(bool success, const QString &error);

private:
    void setupUi();
    void setupVideoOptions();
    void setupImageOptions();

    QString m_inputPath;
    bool m_isVideo;
    bool m_exporting = false;

    // Format/codec
    QComboBox *m_formatCombo = nullptr;

    // Quality
    QSlider *m_qualitySlider = nullptr;
    QLabel *m_qualityLabel = nullptr;

    // Resolution
    QComboBox *m_resolutionCombo = nullptr;
    QSpinBox *m_customWidth = nullptr;
    QSpinBox *m_customHeight = nullptr;

    // Output path
    QLineEdit *m_outputPath = nullptr;
    QPushButton *m_browseButton = nullptr;

    // Progress
    QProgressBar *m_progressBar = nullptr;
    QLabel *m_etaLabel = nullptr;
    QPushButton *m_exportButton = nullptr;
    QPushButton *m_cancelButton = nullptr;

    // Export tracking
    QTimer *m_progressPollTimer = nullptr;
};

#endif // EXPORTDIALOG_H
```

- [ ] **Step 2: Create ExportDialog implementation**

Create `qt/src/ExportDialog.cpp`:
```cpp
#include "ExportDialog.h"
#include "RustBridge.h"
#include "AppState.h"

#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QGridLayout>
#include <QGroupBox>
#include <QFileDialog>
#include <QFileInfo>
#include <QMessageBox>
#include <QStandardPaths>

ExportDialog::ExportDialog(const QString &inputPath, bool isVideo, QWidget *parent)
    : QDialog(parent)
    , m_inputPath(inputPath)
    , m_isVideo(isVideo)
{
    setWindowTitle(tr("Export"));
    setMinimumWidth(450);
    setModal(true);
    setupUi();
}

void ExportDialog::setupUi()
{
    auto *mainLayout = new QVBoxLayout(this);

    // --- Format ---
    auto *formatGroup = new QGroupBox(tr("Format"), this);
    auto *formatLayout = new QHBoxLayout(formatGroup);
    m_formatCombo = new QComboBox(this);
    if (m_isVideo) {
        m_formatCombo->addItem(tr("MP4 (H.264)"), "mp4");
        m_formatCombo->addItem(tr("WebM (VP9)"), "webm");
        m_formatCombo->addItem(tr("GIF"), "gif");
    } else {
        m_formatCombo->addItem(tr("PNG"), "png");
        m_formatCombo->addItem(tr("JPEG"), "jpeg");
        m_formatCombo->addItem(tr("WebP"), "webp");
    }
    formatLayout->addWidget(new QLabel(tr("Format:"), this));
    formatLayout->addWidget(m_formatCombo);
    formatLayout->addStretch();
    mainLayout->addWidget(formatGroup);

    // --- Quality ---
    auto *qualityGroup = new QGroupBox(tr("Quality"), this);
    auto *qualityLayout = new QHBoxLayout(qualityGroup);
    qualityLayout->addWidget(new QLabel(tr("Quality:"), this));
    m_qualitySlider = new QSlider(Qt::Horizontal, this);
    if (m_isVideo) {
        // CRF: 0 (best) to 51 (worst), inverted for user
        m_qualitySlider->setRange(0, 51);
        m_qualitySlider->setValue(23); // default CRF
        m_qualitySlider->setInvertedAppearance(true);
    } else {
        m_qualitySlider->setRange(1, 100);
        m_qualitySlider->setValue(95);
    }
    qualityLayout->addWidget(m_qualitySlider);
    m_qualityLabel = new QLabel(this);
    m_qualityLabel->setMinimumWidth(40);
    qualityLayout->addWidget(m_qualityLabel);

    connect(m_qualitySlider, &QSlider::valueChanged, this, [this](int val) {
        if (m_isVideo)
            m_qualityLabel->setText(QString("CRF %1").arg(val));
        else
            m_qualityLabel->setText(QString("%1%").arg(val));
    });
    m_qualitySlider->valueChanged(m_qualitySlider->value()); // trigger initial label

    mainLayout->addWidget(qualityGroup);

    // --- Resolution (video only) ---
    if (m_isVideo) {
        auto *resGroup = new QGroupBox(tr("Resolution"), this);
        auto *resLayout = new QHBoxLayout(resGroup);
        m_resolutionCombo = new QComboBox(this);
        m_resolutionCombo->addItem(tr("Native (original)"), "native");
        m_resolutionCombo->addItem(tr("1080p"), "1080p");
        m_resolutionCombo->addItem(tr("720p"), "720p");
        m_resolutionCombo->addItem(tr("Custom"), "custom");
        resLayout->addWidget(m_resolutionCombo);

        m_customWidth = new QSpinBox(this);
        m_customWidth->setRange(64, 7680);
        m_customWidth->setValue(1920);
        m_customWidth->setPrefix(tr("W: "));
        m_customWidth->setVisible(false);

        m_customHeight = new QSpinBox(this);
        m_customHeight->setRange(64, 4320);
        m_customHeight->setValue(1080);
        m_customHeight->setPrefix(tr("H: "));
        m_customHeight->setVisible(false);

        resLayout->addWidget(m_customWidth);
        resLayout->addWidget(m_customHeight);
        resLayout->addStretch();

        connect(m_resolutionCombo, &QComboBox::currentTextChanged, this, [this]() {
            bool custom = (m_resolutionCombo->currentData().toString() == "custom");
            m_customWidth->setVisible(custom);
            m_customHeight->setVisible(custom);
        });

        mainLayout->addWidget(resGroup);
    }

    // --- Output path ---
    auto *outputGroup = new QGroupBox(tr("Output"), this);
    auto *outputLayout = new QHBoxLayout(outputGroup);
    m_outputPath = new QLineEdit(this);

    // Default output path
    QString ext = m_formatCombo->currentData().toString();
    QString defaultDir = AppState::instance().settings().export_settings.output_directory;
    if (defaultDir.isEmpty()) {
        defaultDir = QStandardPaths::writableLocation(QStandardPaths::MoviesLocation);
    }
    QString baseName = QFileInfo(m_inputPath).completeBaseName();
    m_outputPath->setText(QString("%1/%2_export.%3").arg(defaultDir, baseName, ext));

    outputLayout->addWidget(m_outputPath);
    m_browseButton = new QPushButton(tr("Browse..."), this);
    connect(m_browseButton, &QPushButton::clicked, this, &ExportDialog::onBrowseOutput);
    outputLayout->addWidget(m_browseButton);
    mainLayout->addWidget(outputGroup);

    // Update extension when format changes
    connect(m_formatCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, [this]() {
        QString path = m_outputPath->text();
        QString newExt = m_formatCombo->currentData().toString();
        int dotIdx = path.lastIndexOf('.');
        if (dotIdx > 0) {
            m_outputPath->setText(path.left(dotIdx + 1) + newExt);
        }
    });

    // --- Progress ---
    m_progressBar = new QProgressBar(this);
    m_progressBar->setRange(0, 100);
    m_progressBar->setValue(0);
    m_progressBar->setVisible(false);
    mainLayout->addWidget(m_progressBar);

    m_etaLabel = new QLabel(this);
    m_etaLabel->setVisible(false);
    m_etaLabel->setStyleSheet("QLabel { color: #999; font-size: 11px; }");
    mainLayout->addWidget(m_etaLabel);

    // --- Buttons ---
    auto *buttonLayout = new QHBoxLayout();
    buttonLayout->addStretch();
    m_cancelButton = new QPushButton(tr("Cancel"), this);
    connect(m_cancelButton, &QPushButton::clicked, this, &ExportDialog::onCancel);
    buttonLayout->addWidget(m_cancelButton);

    m_exportButton = new QPushButton(tr("Export"), this);
    m_exportButton->setDefault(true);
    m_exportButton->setStyleSheet(
        "QPushButton { background-color: #0A84FF; color: white; "
        "padding: 6px 20px; border-radius: 4px; font-weight: bold; }");
    connect(m_exportButton, &QPushButton::clicked, this, &ExportDialog::onExport);
    buttonLayout->addWidget(m_exportButton);
    mainLayout->addLayout(buttonLayout);
}

void ExportDialog::onBrowseOutput()
{
    QString filter;
    if (m_isVideo)
        filter = tr("Video Files (*.mp4 *.webm *.gif);;All Files (*)");
    else
        filter = tr("Image Files (*.png *.jpeg *.jpg *.webp);;All Files (*)");

    QString path = QFileDialog::getSaveFileName(this, tr("Export As"),
                                                 m_outputPath->text(), filter);
    if (!path.isEmpty()) {
        m_outputPath->setText(path);
    }
}

void ExportDialog::onExport()
{
    if (m_exporting) return;

    QString outputPath = m_outputPath->text();
    if (outputPath.isEmpty()) {
        QMessageBox::warning(this, tr("Export"), tr("Please specify an output path."));
        return;
    }

    m_exporting = true;
    m_exportButton->setEnabled(false);
    m_progressBar->setVisible(true);
    m_progressBar->setValue(0);
    m_etaLabel->setVisible(true);
    m_etaLabel->setText(tr("Starting export..."));

    // Build export parameters
    auto &bridge = RustBridge::instance();

    QString format = m_formatCombo->currentData().toString();
    int quality = m_qualitySlider->value();

    QString resolution = "native";
    if (m_resolutionCombo) {
        resolution = m_resolutionCombo->currentData().toString();
        if (resolution == "custom") {
            resolution = QString("%1x%2")
                .arg(m_customWidth->value())
                .arg(m_customHeight->value());
        }
    }

    // Start export via RustBridge (runs FFmpeg in background)
    bool started = bridge.startExport(
        m_inputPath, outputPath, format, quality, resolution);

    if (!started) {
        onExportComplete(false, tr("Failed to start export process."));
        return;
    }

    // Poll progress
    m_progressPollTimer = new QTimer(this);
    m_progressPollTimer->setInterval(250);
    connect(m_progressPollTimer, &QTimer::timeout, this, [this]() {
        auto &bridge = RustBridge::instance();
        int percent = bridge.exportProgress();
        QString eta = bridge.exportEta();

        onExportProgress(percent, eta);

        if (percent >= 100 || percent < 0) {
            m_progressPollTimer->stop();
            bool success = (percent >= 100);
            QString error = success ? QString() : bridge.exportError();
            onExportComplete(success, error);
        }
    });
    m_progressPollTimer->start();
}

void ExportDialog::onCancel()
{
    if (m_exporting) {
        RustBridge::instance().cancelExport();
        if (m_progressPollTimer) m_progressPollTimer->stop();
        m_exporting = false;
        m_exportButton->setEnabled(true);
        m_progressBar->setVisible(false);
        m_etaLabel->setVisible(false);
    } else {
        reject();
    }
}

void ExportDialog::onExportProgress(int percent, const QString &eta)
{
    m_progressBar->setValue(percent);
    m_etaLabel->setText(tr("Exporting... %1% — ETA: %2").arg(percent).arg(eta));
}

void ExportDialog::onExportComplete(bool success, const QString &error)
{
    m_exporting = false;
    m_exportButton->setEnabled(true);

    if (success) {
        m_progressBar->setValue(100);
        m_etaLabel->setText(tr("Export complete!"));
        QMessageBox::information(this, tr("Export"),
            tr("Export completed successfully.\n%1").arg(m_outputPath->text()));
        accept();
    } else {
        m_progressBar->setVisible(false);
        m_etaLabel->setVisible(false);
        QMessageBox::critical(this, tr("Export Failed"),
            tr("Export failed: %1").arg(error));
    }
}
```

- [ ] **Step 3: Add to CMakeLists.txt and verify**

Add to the `SOURCES` list:
```cmake
src/ExportDialog.h
src/ExportDialog.cpp
```

```bash
cd /home/rw3iss/Sites/others/screen-recorder/qt
cmake --build build --target ScreenDream 2>&1 | tail -20
```

---

## Task 28: SettingsDialog

**Files:**
- Create: `qt/src/SettingsDialog.h`
- Create: `qt/src/SettingsDialog.cpp`
- Modify: `qt/CMakeLists.txt`

- [ ] **Step 1: Create SettingsDialog header**

Create `qt/src/SettingsDialog.h`:
```cpp
#ifndef SETTINGSDIALOG_H
#define SETTINGSDIALOG_H

#include <QDialog>
#include <QListWidget>
#include <QStackedWidget>
#include <QPushButton>

// Forward declarations for page widgets
class QCheckBox;
class QSpinBox;
class QComboBox;
class QSlider;
class QLineEdit;
class QKeySequenceEdit;
class QLabel;

/// Settings dialog with category list + stacked pages.
///
/// Categories: General, Recording, Screenshot, Export, Shortcuts, FFmpeg, About
///
/// Loads settings from JSON on open, saves on Apply/OK.
class SettingsDialog : public QDialog {
    Q_OBJECT

public:
    explicit SettingsDialog(QWidget *parent = nullptr);
    ~SettingsDialog() override = default;

private slots:
    void onCategoryChanged(int row);
    void onOk();
    void onApply();
    void onBrowseOutputDir();
    void onBrowseFfmpegPath();

private:
    void setupUi();
    QWidget *createGeneralPage();
    QWidget *createRecordingPage();
    QWidget *createScreenshotPage();
    QWidget *createExportPage();
    QWidget *createShortcutsPage();
    QWidget *createFfmpegPage();
    QWidget *createAboutPage();

    void loadSettings();
    void saveSettings();

    QListWidget *m_categoryList = nullptr;
    QStackedWidget *m_pageStack = nullptr;

    // General page
    QCheckBox *m_minimizeToTray = nullptr;
    QCheckBox *m_startMinimized = nullptr;
    QComboBox *m_themeSelector = nullptr;

    // Recording page
    QSpinBox *m_fpsSpinBox = nullptr;
    QComboBox *m_codecCombo = nullptr;
    QComboBox *m_presetCombo = nullptr;
    QSlider *m_crfSlider = nullptr;
    QLabel *m_crfLabel = nullptr;

    // Screenshot page
    QComboBox *m_screenshotFormat = nullptr;
    QSpinBox *m_screenshotQuality = nullptr;
    QCheckBox *m_copyToClipboard = nullptr;
    QCheckBox *m_saveToDisk = nullptr;

    // Export page
    QLineEdit *m_outputDir = nullptr;

    // Shortcuts page
    QKeySequenceEdit *m_shortcutStartStop = nullptr;
    QKeySequenceEdit *m_shortcutPauseResume = nullptr;
    QKeySequenceEdit *m_shortcutScreenshot = nullptr;

    // FFmpeg page
    QComboBox *m_ffmpegSource = nullptr;
    QLineEdit *m_ffmpegCustomPath = nullptr;
    QLabel *m_ffmpegVersion = nullptr;

    // Buttons
    QPushButton *m_okButton = nullptr;
    QPushButton *m_applyButton = nullptr;
    QPushButton *m_cancelButton = nullptr;
};

#endif // SETTINGSDIALOG_H
```

- [ ] **Step 2: Create SettingsDialog implementation**

Create `qt/src/SettingsDialog.cpp`:
```cpp
#include "SettingsDialog.h"
#include "AppState.h"
#include "RustBridge.h"

#include <QHBoxLayout>
#include <QVBoxLayout>
#include <QGridLayout>
#include <QGroupBox>
#include <QCheckBox>
#include <QSpinBox>
#include <QComboBox>
#include <QSlider>
#include <QLineEdit>
#include <QKeySequenceEdit>
#include <QLabel>
#include <QFileDialog>
#include <QPushButton>
#include <QTextEdit>

SettingsDialog::SettingsDialog(QWidget *parent)
    : QDialog(parent)
{
    setWindowTitle(tr("Settings"));
    setMinimumSize(700, 500);
    resize(750, 550);
    setupUi();
    loadSettings();
}

void SettingsDialog::setupUi()
{
    auto *mainLayout = new QHBoxLayout(this);

    // --- Category list (left) ---
    m_categoryList = new QListWidget(this);
    m_categoryList->setFixedWidth(160);
    m_categoryList->setSpacing(2);
    m_categoryList->addItem(tr("General"));
    m_categoryList->addItem(tr("Recording"));
    m_categoryList->addItem(tr("Screenshot"));
    m_categoryList->addItem(tr("Export"));
    m_categoryList->addItem(tr("Shortcuts"));
    m_categoryList->addItem(tr("FFmpeg"));
    m_categoryList->addItem(tr("About"));
    mainLayout->addWidget(m_categoryList);

    // --- Right side: stacked pages + buttons ---
    auto *rightLayout = new QVBoxLayout();

    m_pageStack = new QStackedWidget(this);
    m_pageStack->addWidget(createGeneralPage());
    m_pageStack->addWidget(createRecordingPage());
    m_pageStack->addWidget(createScreenshotPage());
    m_pageStack->addWidget(createExportPage());
    m_pageStack->addWidget(createShortcutsPage());
    m_pageStack->addWidget(createFfmpegPage());
    m_pageStack->addWidget(createAboutPage());
    rightLayout->addWidget(m_pageStack, 1);

    // --- Buttons ---
    auto *buttonLayout = new QHBoxLayout();
    buttonLayout->addStretch();
    m_okButton = new QPushButton(tr("OK"), this);
    m_applyButton = new QPushButton(tr("Apply"), this);
    m_cancelButton = new QPushButton(tr("Cancel"), this);
    buttonLayout->addWidget(m_okButton);
    buttonLayout->addWidget(m_applyButton);
    buttonLayout->addWidget(m_cancelButton);
    rightLayout->addLayout(buttonLayout);

    mainLayout->addLayout(rightLayout, 1);

    // --- Connections ---
    connect(m_categoryList, &QListWidget::currentRowChanged,
            this, &SettingsDialog::onCategoryChanged);
    connect(m_okButton, &QPushButton::clicked, this, &SettingsDialog::onOk);
    connect(m_applyButton, &QPushButton::clicked, this, &SettingsDialog::onApply);
    connect(m_cancelButton, &QPushButton::clicked, this, &QDialog::reject);

    m_categoryList->setCurrentRow(0);
}

void SettingsDialog::onCategoryChanged(int row)
{
    m_pageStack->setCurrentIndex(row);
}

// --- Page creation methods ---

QWidget *SettingsDialog::createGeneralPage()
{
    auto *page = new QWidget(this);
    auto *layout = new QVBoxLayout(page);

    auto *group = new QGroupBox(tr("General Settings"), page);
    auto *grid = new QGridLayout(group);

    m_minimizeToTray = new QCheckBox(tr("Minimize to system tray on close"), page);
    grid->addWidget(m_minimizeToTray, 0, 0, 1, 2);

    m_startMinimized = new QCheckBox(tr("Start minimized"), page);
    grid->addWidget(m_startMinimized, 1, 0, 1, 2);

    grid->addWidget(new QLabel(tr("Theme:"), page), 2, 0);
    m_themeSelector = new QComboBox(page);
    m_themeSelector->addItem(tr("Dark"));
    m_themeSelector->addItem(tr("Light"));
    m_themeSelector->addItem(tr("System"));
    grid->addWidget(m_themeSelector, 2, 1);

    layout->addWidget(group);
    layout->addStretch();
    return page;
}

QWidget *SettingsDialog::createRecordingPage()
{
    auto *page = new QWidget(this);
    auto *layout = new QVBoxLayout(page);

    auto *group = new QGroupBox(tr("Recording Settings"), page);
    auto *grid = new QGridLayout(group);

    // FPS
    grid->addWidget(new QLabel(tr("Frame Rate (FPS):"), page), 0, 0);
    m_fpsSpinBox = new QSpinBox(page);
    m_fpsSpinBox->setRange(1, 120);
    m_fpsSpinBox->setValue(30);
    m_fpsSpinBox->setSuffix(" fps");
    grid->addWidget(m_fpsSpinBox, 0, 1);

    // Codec
    grid->addWidget(new QLabel(tr("Video Codec:"), page), 1, 0);
    m_codecCombo = new QComboBox(page);
    m_codecCombo->addItem("H.264 (libx264)", "h264");
    m_codecCombo->addItem("H.265 (libx265)", "h265");
    m_codecCombo->addItem("VP9 (libvpx-vp9)", "vp9");
    grid->addWidget(m_codecCombo, 1, 1);

    // Preset
    grid->addWidget(new QLabel(tr("Encoding Preset:"), page), 2, 0);
    m_presetCombo = new QComboBox(page);
    m_presetCombo->addItems({"ultrafast", "superfast", "veryfast", "faster",
                              "fast", "medium", "slow", "slower", "veryslow"});
    m_presetCombo->setCurrentText("fast");
    grid->addWidget(m_presetCombo, 2, 1);

    // CRF
    grid->addWidget(new QLabel(tr("Quality (CRF):"), page), 3, 0);
    auto *crfLayout = new QHBoxLayout();
    m_crfSlider = new QSlider(Qt::Horizontal, page);
    m_crfSlider->setRange(0, 51);
    m_crfSlider->setValue(23);
    m_crfLabel = new QLabel("23", page);
    m_crfLabel->setMinimumWidth(30);
    connect(m_crfSlider, &QSlider::valueChanged, this, [this](int val) {
        m_crfLabel->setText(QString::number(val));
    });
    crfLayout->addWidget(m_crfSlider);
    crfLayout->addWidget(m_crfLabel);
    grid->addLayout(crfLayout, 3, 1);

    // Info label
    auto *info = new QLabel(tr("Lower CRF = better quality, larger file. "
                                "Recommended: 18-28."), page);
    info->setStyleSheet("QLabel { color: #888; font-size: 11px; }");
    info->setWordWrap(true);
    grid->addWidget(info, 4, 0, 1, 2);

    layout->addWidget(group);
    layout->addStretch();
    return page;
}

QWidget *SettingsDialog::createScreenshotPage()
{
    auto *page = new QWidget(this);
    auto *layout = new QVBoxLayout(page);

    auto *group = new QGroupBox(tr("Screenshot Settings"), page);
    auto *grid = new QGridLayout(group);

    grid->addWidget(new QLabel(tr("Format:"), page), 0, 0);
    m_screenshotFormat = new QComboBox(page);
    m_screenshotFormat->addItem("PNG", "png");
    m_screenshotFormat->addItem("JPEG", "jpeg");
    m_screenshotFormat->addItem("WebP", "webp");
    grid->addWidget(m_screenshotFormat, 0, 1);

    grid->addWidget(new QLabel(tr("Quality:"), page), 1, 0);
    m_screenshotQuality = new QSpinBox(page);
    m_screenshotQuality->setRange(1, 100);
    m_screenshotQuality->setValue(100);
    m_screenshotQuality->setSuffix("%");
    grid->addWidget(m_screenshotQuality, 1, 1);

    m_copyToClipboard = new QCheckBox(tr("Copy to clipboard after capture"), page);
    grid->addWidget(m_copyToClipboard, 2, 0, 1, 2);

    m_saveToDisk = new QCheckBox(tr("Save to disk after capture"), page);
    grid->addWidget(m_saveToDisk, 3, 0, 1, 2);

    layout->addWidget(group);
    layout->addStretch();
    return page;
}

QWidget *SettingsDialog::createExportPage()
{
    auto *page = new QWidget(this);
    auto *layout = new QVBoxLayout(page);

    auto *group = new QGroupBox(tr("Export Settings"), page);
    auto *grid = new QGridLayout(group);

    grid->addWidget(new QLabel(tr("Output Directory:"), page), 0, 0);

    auto *pathLayout = new QHBoxLayout();
    m_outputDir = new QLineEdit(page);
    pathLayout->addWidget(m_outputDir);
    auto *browseBtn = new QPushButton(tr("Browse..."), page);
    connect(browseBtn, &QPushButton::clicked, this, &SettingsDialog::onBrowseOutputDir);
    pathLayout->addWidget(browseBtn);
    grid->addLayout(pathLayout, 0, 1);

    layout->addWidget(group);
    layout->addStretch();
    return page;
}

QWidget *SettingsDialog::createShortcutsPage()
{
    auto *page = new QWidget(this);
    auto *layout = new QVBoxLayout(page);

    auto *group = new QGroupBox(tr("Keyboard Shortcuts"), page);
    auto *grid = new QGridLayout(group);

    grid->addWidget(new QLabel(tr("Start/Stop Recording:"), page), 0, 0);
    m_shortcutStartStop = new QKeySequenceEdit(page);
    grid->addWidget(m_shortcutStartStop, 0, 1);

    grid->addWidget(new QLabel(tr("Pause/Resume Recording:"), page), 1, 0);
    m_shortcutPauseResume = new QKeySequenceEdit(page);
    grid->addWidget(m_shortcutPauseResume, 1, 1);

    grid->addWidget(new QLabel(tr("Take Screenshot:"), page), 2, 0);
    m_shortcutScreenshot = new QKeySequenceEdit(page);
    grid->addWidget(m_shortcutScreenshot, 2, 1);

    auto *note = new QLabel(
        tr("These shortcuts work globally (when the app is not focused) "
           "if your platform supports it. On Wayland, global shortcuts "
           "may require portal permissions."), page);
    note->setStyleSheet("QLabel { color: #888; font-size: 11px; }");
    note->setWordWrap(true);
    grid->addWidget(note, 3, 0, 1, 2);

    layout->addWidget(group);
    layout->addStretch();
    return page;
}

QWidget *SettingsDialog::createFfmpegPage()
{
    auto *page = new QWidget(this);
    auto *layout = new QVBoxLayout(page);

    auto *group = new QGroupBox(tr("FFmpeg Configuration"), page);
    auto *grid = new QGridLayout(group);

    grid->addWidget(new QLabel(tr("FFmpeg Source:"), page), 0, 0);
    m_ffmpegSource = new QComboBox(page);
    m_ffmpegSource->addItem(tr("Bundled"), "bundled");
    m_ffmpegSource->addItem(tr("System (PATH)"), "system");
    m_ffmpegSource->addItem(tr("Custom Path"), "custom");
    grid->addWidget(m_ffmpegSource, 0, 1);

    grid->addWidget(new QLabel(tr("Custom Path:"), page), 1, 0);
    auto *pathLayout = new QHBoxLayout();
    m_ffmpegCustomPath = new QLineEdit(page);
    m_ffmpegCustomPath->setPlaceholderText(tr("/usr/bin/ffmpeg"));
    m_ffmpegCustomPath->setEnabled(false);
    pathLayout->addWidget(m_ffmpegCustomPath);
    auto *browseBtn = new QPushButton(tr("Browse..."), page);
    connect(browseBtn, &QPushButton::clicked, this, &SettingsDialog::onBrowseFfmpegPath);
    pathLayout->addWidget(browseBtn);
    grid->addLayout(pathLayout, 1, 1);

    // Enable/disable custom path based on source selection
    connect(m_ffmpegSource, &QComboBox::currentTextChanged, this, [this]() {
        bool isCustom = (m_ffmpegSource->currentData().toString() == "custom");
        m_ffmpegCustomPath->setEnabled(isCustom);
    });

    // Detected version label
    grid->addWidget(new QLabel(tr("Detected Version:"), page), 2, 0);
    m_ffmpegVersion = new QLabel(page);
    m_ffmpegVersion->setStyleSheet("QLabel { color: #0A84FF; }");
    grid->addWidget(m_ffmpegVersion, 2, 1);

    // Query FFmpeg version
    QString version = RustBridge::instance().ffmpegVersion();
    m_ffmpegVersion->setText(version.isEmpty() ? tr("Not found") : version);

    layout->addWidget(group);
    layout->addStretch();
    return page;
}

QWidget *SettingsDialog::createAboutPage()
{
    auto *page = new QWidget(this);
    auto *layout = new QVBoxLayout(page);

    auto *title = new QLabel(tr("Screen Dream"), page);
    title->setStyleSheet("QLabel { font-size: 24px; font-weight: bold; }");
    layout->addWidget(title);

    auto *version = new QLabel(tr("Version 1.0.0"), page);
    version->setStyleSheet("QLabel { font-size: 14px; color: #888; }");
    layout->addWidget(version);

    layout->addSpacing(16);

    auto *desc = new QLabel(
        tr("A cross-platform screen recording, screenshot, and media editing application.\n\n"
           "Built with Qt 6 and Rust."), page);
    desc->setWordWrap(true);
    layout->addWidget(desc);

    layout->addSpacing(16);

    auto *licenseLabel = new QLabel(tr("License:"), page);
    licenseLabel->setStyleSheet("QLabel { font-weight: bold; }");
    layout->addWidget(licenseLabel);

    auto *licenseText = new QTextEdit(page);
    licenseText->setReadOnly(true);
    licenseText->setPlainText(
        "This program is free software: you can redistribute it and/or modify\n"
        "it under the terms of the GNU General Public License as published by\n"
        "the Free Software Foundation, either version 3 of the License, or\n"
        "(at your option) any later version.\n\n"
        "This program is distributed in the hope that it will be useful,\n"
        "but WITHOUT ANY WARRANTY; without even the implied warranty of\n"
        "MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the\n"
        "GNU General Public License for more details.");
    licenseText->setMaximumHeight(150);
    layout->addWidget(licenseText);

    layout->addStretch();
    return page;
}

// --- Load / Save ---

void SettingsDialog::loadSettings()
{
    auto &settings = AppState::instance().settings();

    // General
    m_minimizeToTray->setChecked(settings.general.minimize_to_tray);
    m_startMinimized->setChecked(settings.general.start_minimized);
    // Theme: not yet in settings model, default to Dark

    // Recording
    m_fpsSpinBox->setValue(settings.recording.fps);
    // Match codec string to combo index
    QString codec = settings.recording.video_codec;
    for (int i = 0; i < m_codecCombo->count(); ++i) {
        if (m_codecCombo->itemData(i).toString() == codec) {
            m_codecCombo->setCurrentIndex(i);
            break;
        }
    }
    m_presetCombo->setCurrentText(settings.recording.preset);
    m_crfSlider->setValue(settings.recording.crf);

    // Screenshot
    QString screenshotFmt = settings.screenshot.format;
    for (int i = 0; i < m_screenshotFormat->count(); ++i) {
        if (m_screenshotFormat->itemData(i).toString() == screenshotFmt) {
            m_screenshotFormat->setCurrentIndex(i);
            break;
        }
    }
    m_screenshotQuality->setValue(settings.screenshot.quality);
    m_copyToClipboard->setChecked(settings.screenshot.copy_to_clipboard);
    m_saveToDisk->setChecked(settings.screenshot.save_to_disk);

    // Export
    m_outputDir->setText(settings.export_settings.output_directory);

    // Shortcuts — convert from Electron-style to Qt key sequences
    auto parseKey = [](const QString &s) -> QKeySequence {
        QString q = s;
        q.replace("CommandOrControl", "Ctrl");
        q.replace("Command", "Meta");
        q.replace("Control", "Ctrl");
        return QKeySequence(q);
    };
    m_shortcutStartStop->setKeySequence(
        parseKey(settings.shortcuts.start_stop_recording));
    m_shortcutPauseResume->setKeySequence(
        parseKey(settings.shortcuts.pause_resume_recording));
    m_shortcutScreenshot->setKeySequence(
        parseKey(settings.shortcuts.take_screenshot));

    // FFmpeg
    QString src = settings.ffmpeg.source;
    for (int i = 0; i < m_ffmpegSource->count(); ++i) {
        if (m_ffmpegSource->itemData(i).toString() == src) {
            m_ffmpegSource->setCurrentIndex(i);
            break;
        }
    }
    m_ffmpegCustomPath->setText(settings.ffmpeg.custom_path);
}

void SettingsDialog::saveSettings()
{
    auto &state = AppState::instance();

    // Build a settings update struct and apply via AppState
    // (AppState writes to JSON through RustBridge)

    // General
    state.setSetting("general.minimize_to_tray", m_minimizeToTray->isChecked());
    state.setSetting("general.start_minimized", m_startMinimized->isChecked());

    // Recording
    state.setSetting("recording.fps", m_fpsSpinBox->value());
    state.setSetting("recording.video_codec", m_codecCombo->currentData().toString());
    state.setSetting("recording.preset", m_presetCombo->currentText());
    state.setSetting("recording.crf", m_crfSlider->value());

    // Screenshot
    state.setSetting("screenshot.format", m_screenshotFormat->currentData().toString());
    state.setSetting("screenshot.quality", m_screenshotQuality->value());
    state.setSetting("screenshot.copy_to_clipboard", m_copyToClipboard->isChecked());
    state.setSetting("screenshot.save_to_disk", m_saveToDisk->isChecked());

    // Export
    state.setSetting("export.output_directory", m_outputDir->text());

    // Shortcuts — convert back to Electron-style format for Rust compatibility
    auto toSettingsStr = [](const QKeySequence &seq) -> QString {
        QString s = seq.toString();
        s.replace("Ctrl", "CommandOrControl");
        s.replace("Meta", "Command");
        return s;
    };
    state.setSetting("shortcuts.start_stop_recording",
                     toSettingsStr(m_shortcutStartStop->keySequence()));
    state.setSetting("shortcuts.pause_resume_recording",
                     toSettingsStr(m_shortcutPauseResume->keySequence()));
    state.setSetting("shortcuts.take_screenshot",
                     toSettingsStr(m_shortcutScreenshot->keySequence()));

    // FFmpeg
    state.setSetting("ffmpeg.source", m_ffmpegSource->currentData().toString());
    state.setSetting("ffmpeg.custom_path", m_ffmpegCustomPath->text());

    // Persist to disk
    state.saveSettings();
}

void SettingsDialog::onOk()
{
    saveSettings();
    accept();
}

void SettingsDialog::onApply()
{
    saveSettings();
}

void SettingsDialog::onBrowseOutputDir()
{
    QString dir = QFileDialog::getExistingDirectory(
        this, tr("Select Output Directory"), m_outputDir->text());
    if (!dir.isEmpty()) {
        m_outputDir->setText(dir);
    }
}

void SettingsDialog::onBrowseFfmpegPath()
{
    QString path = QFileDialog::getOpenFileName(
        this, tr("Select FFmpeg Binary"), "/usr/bin",
        tr("Executables (*)"));
    if (!path.isEmpty()) {
        m_ffmpegCustomPath->setText(path);
    }
}
```

- [ ] **Step 3: Add to CMakeLists.txt and verify**

Add to the `SOURCES` list:
```cmake
src/SettingsDialog.h
src/SettingsDialog.cpp
```

```bash
cd /home/rw3iss/Sites/others/screen-recorder/qt
cmake --build build --target ScreenDream 2>&1 | tail -20
```

---

## Task 29: Build & Packaging

**Files:**
- Create: `qt/packaging/appimage/build-appimage.sh`
- Create: `qt/packaging/macos/build-dmg.sh`
- Create: `qt/packaging/windows/installer.nsi`
- Create: `qt/resources/screendream.desktop`
- Modify: `qt/CMakeLists.txt`

- [ ] **Step 1: Create Linux .desktop file**

Create `qt/resources/screendream.desktop`:
```ini
[Desktop Entry]
Type=Application
Name=Screen Dream
GenericName=Screen Recorder
Comment=Screen recording, screenshots, and media editing
Exec=screendream %U
Icon=screendream
Terminal=false
Categories=AudioVideo;Video;Graphics;Utility;
Keywords=screen;record;capture;screenshot;video;editor;
MimeType=video/mp4;video/webm;image/png;image/jpeg;
StartupWMClass=screendream
StartupNotify=true
```

- [ ] **Step 2: Create AppImage build script (Linux)**

Create `qt/packaging/appimage/build-appimage.sh`:
```bash
#!/usr/bin/env bash
set -euo pipefail

# Build AppImage for Screen Dream
# Requires: linuxdeployqt or appimage-builder, appimagetool

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="$PROJECT_ROOT/build-release"
APPDIR="$BUILD_DIR/AppDir"
OUTPUT_DIR="$PROJECT_ROOT/dist"

echo "=== Building Screen Dream AppImage ==="

# Step 1: Build the project in Release mode
echo "[1/5] Building Qt project..."
mkdir -p "$BUILD_DIR"
cd "$BUILD_DIR"
cmake "$PROJECT_ROOT" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX=/usr
cmake --build . --parallel "$(nproc)"

# Step 2: Create AppDir structure
echo "[2/5] Creating AppDir..."
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/lib"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/256x256/apps"

# Copy binary
cp "$BUILD_DIR/ScreenDream" "$APPDIR/usr/bin/screendream"

# Copy Rust shared library
cp "$PROJECT_ROOT/../src-tauri/target/release/libsd_core.so" "$APPDIR/usr/lib/" 2>/dev/null || \
    echo "WARNING: libsd_core.so not found in release build"

# Copy desktop file and icon
cp "$PROJECT_ROOT/resources/screendream.desktop" "$APPDIR/usr/share/applications/"
cp "$PROJECT_ROOT/resources/icons/screendream.png" \
    "$APPDIR/usr/share/icons/hicolor/256x256/apps/" 2>/dev/null || \
    echo "WARNING: screendream.png icon not found"

# Symlinks for AppImage convention
ln -sf "usr/share/applications/screendream.desktop" "$APPDIR/screendream.desktop"
ln -sf "usr/share/icons/hicolor/256x256/apps/screendream.png" "$APPDIR/screendream.png"

# Step 3: Bundle FFmpeg
echo "[3/5] Bundling FFmpeg..."
FFMPEG_BIN="$(which ffmpeg 2>/dev/null || echo '')"
if [ -n "$FFMPEG_BIN" ]; then
    cp "$FFMPEG_BIN" "$APPDIR/usr/bin/ffmpeg"
    # Also copy ffprobe
    FFPROBE_BIN="$(which ffprobe 2>/dev/null || echo '')"
    if [ -n "$FFPROBE_BIN" ]; then
        cp "$FFPROBE_BIN" "$APPDIR/usr/bin/ffprobe"
    fi
else
    echo "WARNING: ffmpeg not found in PATH. AppImage will rely on system ffmpeg."
fi

# Step 4: Deploy Qt libraries
echo "[4/5] Deploying Qt libraries..."
if command -v linuxdeployqt &>/dev/null; then
    linuxdeployqt "$APPDIR/usr/share/applications/screendream.desktop" \
        -bundle-non-qt-libs \
        -no-translations \
        -extra-plugins=platformthemes/libqgtk3.so
elif command -v linuxdeploy &>/dev/null; then
    linuxdeploy --appdir "$APPDIR" \
        --plugin qt \
        --desktop-file "$APPDIR/usr/share/applications/screendream.desktop"
else
    echo "ERROR: Neither linuxdeployqt nor linuxdeploy found."
    echo "Install one of: linuxdeployqt, linuxdeploy-x86_64.AppImage"
    exit 1
fi

# Step 5: Create AppImage
echo "[5/5] Creating AppImage..."
mkdir -p "$OUTPUT_DIR"
if command -v appimagetool &>/dev/null; then
    ARCH=x86_64 appimagetool "$APPDIR" "$OUTPUT_DIR/ScreenDream-x86_64.AppImage"
else
    echo "ERROR: appimagetool not found."
    exit 1
fi

echo ""
echo "=== AppImage created: $OUTPUT_DIR/ScreenDream-x86_64.AppImage ==="
```

```bash
chmod +x qt/packaging/appimage/build-appimage.sh
```

- [ ] **Step 3: Create macOS DMG build script**

Create `qt/packaging/macos/build-dmg.sh`:
```bash
#!/usr/bin/env bash
set -euo pipefail

# Build DMG for Screen Dream (macOS)
# Requires: macdeployqt (comes with Qt), create-dmg (optional)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="$PROJECT_ROOT/build-release"
OUTPUT_DIR="$PROJECT_ROOT/dist"
APP_NAME="Screen Dream"
BUNDLE_NAME="ScreenDream.app"

echo "=== Building Screen Dream DMG ==="

# Step 1: Build
echo "[1/4] Building Qt project..."
mkdir -p "$BUILD_DIR"
cd "$BUILD_DIR"
cmake "$PROJECT_ROOT" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_OSX_DEPLOYMENT_TARGET=12.3
cmake --build . --parallel "$(sysctl -n hw.ncpu)"

# Step 2: Create .app bundle
echo "[2/4] Creating app bundle..."
# macdeployqt creates the bundle and copies Qt frameworks
macdeployqt "$BUILD_DIR/$BUNDLE_NAME" -verbose=1

# Copy Rust shared library into bundle
DYLIB_DIR="$BUILD_DIR/$BUNDLE_NAME/Contents/Frameworks"
mkdir -p "$DYLIB_DIR"
cp "$PROJECT_ROOT/../src-tauri/target/release/libsd_core.dylib" "$DYLIB_DIR/" 2>/dev/null || \
    echo "WARNING: libsd_core.dylib not found in release build"

# Fix rpath for the dylib
install_name_tool -change "libsd_core.dylib" \
    "@executable_path/../Frameworks/libsd_core.dylib" \
    "$BUILD_DIR/$BUNDLE_NAME/Contents/MacOS/ScreenDream" 2>/dev/null || true

# Step 3: Bundle FFmpeg
echo "[3/4] Bundling FFmpeg..."
FFMPEG_BIN="$(which ffmpeg 2>/dev/null || echo '')"
if [ -n "$FFMPEG_BIN" ]; then
    cp "$FFMPEG_BIN" "$BUILD_DIR/$BUNDLE_NAME/Contents/MacOS/ffmpeg"
fi

# Step 4: Create DMG
echo "[4/4] Creating DMG..."
mkdir -p "$OUTPUT_DIR"

if command -v create-dmg &>/dev/null; then
    create-dmg \
        --volname "$APP_NAME" \
        --volicon "$PROJECT_ROOT/resources/icons/screendream.icns" \
        --window-pos 200 120 \
        --window-size 600 400 \
        --icon-size 100 \
        --icon "$BUNDLE_NAME" 150 185 \
        --app-drop-link 450 185 \
        "$OUTPUT_DIR/ScreenDream.dmg" \
        "$BUILD_DIR/$BUNDLE_NAME"
else
    # Fallback: use hdiutil directly
    hdiutil create -volname "$APP_NAME" \
        -srcfolder "$BUILD_DIR/$BUNDLE_NAME" \
        -ov -format UDZO \
        "$OUTPUT_DIR/ScreenDream.dmg"
fi

echo ""
echo "=== DMG created: $OUTPUT_DIR/ScreenDream.dmg ==="
```

```bash
chmod +x qt/packaging/macos/build-dmg.sh
```

- [ ] **Step 4: Create Windows NSIS installer script**

Create `qt/packaging/windows/installer.nsi`:
```nsis
; Screen Dream NSIS Installer Script
; Requires: NSIS 3.x, windeployqt (run before makensis)

!include "MUI2.nsh"

; --- General ---
Name "Screen Dream"
OutFile "..\..\dist\ScreenDream-Setup.exe"
InstallDir "$PROGRAMFILES64\Screen Dream"
InstallDirRegKey HKLM "Software\ScreenDream" "InstallDir"
RequestExecutionLevel admin

; --- UI ---
!define MUI_ABORTWARNING
!define MUI_ICON "..\..\resources\icons\screendream.ico"
!define MUI_UNICON "..\..\resources\icons\screendream.ico"

; --- Pages ---
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\..\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; --- Installation ---
Section "Screen Dream" SecMain
    SetOutPath "$INSTDIR"

    ; Main binary
    File "..\..\build-release\Release\ScreenDream.exe"

    ; Rust shared library
    File "..\..\build-release\Release\sd_core.dll"

    ; Qt libraries (deployed by windeployqt beforehand)
    File /r "..\..\build-release\Release\*.dll"
    File /r "..\..\build-release\Release\platforms"
    File /r "..\..\build-release\Release\styles"
    File /r "..\..\build-release\Release\imageformats"

    ; FFmpeg binary
    File "..\..\packaging\ffmpeg\ffmpeg.exe"
    File "..\..\packaging\ffmpeg\ffprobe.exe"

    ; Desktop shortcut
    CreateShortCut "$DESKTOP\Screen Dream.lnk" "$INSTDIR\ScreenDream.exe" \
        "" "$INSTDIR\ScreenDream.exe" 0

    ; Start menu
    CreateDirectory "$SMPROGRAMS\Screen Dream"
    CreateShortCut "$SMPROGRAMS\Screen Dream\Screen Dream.lnk" "$INSTDIR\ScreenDream.exe"
    CreateShortCut "$SMPROGRAMS\Screen Dream\Uninstall.lnk" "$INSTDIR\Uninstall.exe"

    ; Uninstaller
    WriteUninstaller "$INSTDIR\Uninstall.exe"

    ; Registry keys
    WriteRegStr HKLM "Software\ScreenDream" "InstallDir" "$INSTDIR"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenDream" \
        "DisplayName" "Screen Dream"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenDream" \
        "UninstallString" "$INSTDIR\Uninstall.exe"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenDream" \
        "DisplayIcon" "$INSTDIR\ScreenDream.exe"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenDream" \
        "Publisher" "Screen Dream"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenDream" \
        "DisplayVersion" "1.0.0"
SectionEnd

; --- Uninstallation ---
Section "Uninstall"
    Delete "$INSTDIR\ScreenDream.exe"
    Delete "$INSTDIR\sd_core.dll"
    Delete "$INSTDIR\ffmpeg.exe"
    Delete "$INSTDIR\ffprobe.exe"
    Delete "$INSTDIR\Uninstall.exe"

    ; Remove Qt libraries
    RMDir /r "$INSTDIR\platforms"
    RMDir /r "$INSTDIR\styles"
    RMDir /r "$INSTDIR\imageformats"

    ; Remove shortcuts
    Delete "$DESKTOP\Screen Dream.lnk"
    RMDir /r "$SMPROGRAMS\Screen Dream"

    ; Remove install dir
    RMDir "$INSTDIR"

    ; Registry cleanup
    DeleteRegKey HKLM "Software\ScreenDream"
    DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenDream"
SectionEnd
```

- [ ] **Step 5: Update CMakeLists.txt with install rules and all new sources**

Add to `qt/CMakeLists.txt`:

```cmake
# --- Install rules ---
install(TARGETS ScreenDream RUNTIME DESTINATION bin)
install(FILES resources/screendream.desktop
        DESTINATION share/applications)
install(FILES resources/icons/screendream.png
        DESTINATION share/icons/hicolor/256x256/apps)

# --- Complete source list (add all new files from Tasks 17-28) ---
# Append to existing SOURCES variable:
set(NEW_SOURCES
    src/SourcePickerDialog.h    src/SourcePickerDialog.cpp
    src/RegionSelector.h        src/RegionSelector.cpp
    src/RecorderPanel.h         src/RecorderPanel.cpp
    src/SystemTray.h            src/SystemTray.cpp
    src/GlobalShortcuts.h       src/GlobalShortcuts.cpp
    src/OverlayItem.h           src/OverlayItem.cpp
    src/EditorWindow.h          src/EditorWindow.cpp
    src/VideoPreview.h          src/VideoPreview.cpp
    src/ToolPanel.h             src/ToolPanel.cpp
    src/TimelineWidget.h        src/TimelineWidget.cpp
    src/LayerPanel.h            src/LayerPanel.cpp
    src/ExportDialog.h          src/ExportDialog.cpp
    src/SettingsDialog.h        src/SettingsDialog.cpp
)
target_sources(ScreenDream PRIVATE ${NEW_SOURCES})
```

- [ ] **Step 6: Build and verify the full project**

```bash
# Full Release build
cd /home/rw3iss/Sites/others/screen-recorder/qt
mkdir -p build-release && cd build-release
cmake .. -DCMAKE_BUILD_TYPE=Release
cmake --build . --parallel $(nproc) 2>&1 | tail -30
```

- [ ] **Step 7: Test AppImage packaging (Linux)**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/qt
./packaging/appimage/build-appimage.sh
```

- [ ] **Step 8: Verify .desktop file validates**

```bash
desktop-file-validate qt/resources/screendream.desktop 2>&1 || true
```

---

## Summary of Deliverables

| Phase | Tasks | Key Files |
|-------|-------|-----------|
| **5: Recording Flow** | 17 (SourcePickerDialog), 18 (RegionSelector), 19 (RecorderPanel) | `SourcePickerDialog.h/.cpp`, `RegionSelector.h/.cpp`, `RecorderPanel.h/.cpp` |
| **6: System Integration** | 20 (SystemTray), 21 (GlobalShortcuts) | `SystemTray.h/.cpp`, `GlobalShortcuts.h/.cpp`, `third_party/qhotkey/` |
| **7: Editor, Settings & Packaging** | 22 (EditorWindow), 23 (VideoPreview), 24 (ToolPanel), 25 (Timeline), 26 (LayerPanel), 27 (ExportDialog), 28 (SettingsDialog), 29 (Packaging) | `EditorWindow.h/.cpp`, `VideoPreview.h/.cpp`, `ToolPanel.h/.cpp`, `TimelineWidget.h/.cpp`, `LayerPanel.h/.cpp`, `ExportDialog.h/.cpp`, `SettingsDialog.h/.cpp`, `packaging/*` |

## Build Commands Reference

```bash
# Debug build
cd qt && mkdir -p build && cd build
cmake .. -DCMAKE_BUILD_TYPE=Debug
cmake --build . --parallel $(nproc)

# Release build
cd qt && mkdir -p build-release && cd build-release
cmake .. -DCMAKE_BUILD_TYPE=Release
cmake --build . --parallel $(nproc)

# Run
./build/ScreenDream

# Package (Linux AppImage)
./packaging/appimage/build-appimage.sh

# Package (macOS DMG)
./packaging/macos/build-dmg.sh

# Package (Windows — run in MSVC x64 Native Tools prompt)
windeployqt build-release\Release\ScreenDream.exe
makensis packaging\windows\installer.nsi
```
