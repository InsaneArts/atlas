import SwiftUI

/// An app open in the helper as one group (v3 V9, `.appg`): its real icon, its name and whether it is in front, then
/// its windows front to back with their sizes, or "No windows". Show on Screen shows only on the window row under
/// the pointer.
struct AppCard: View {
  /// One row of the group: the app's head, one of its windows, or the note that it has none.
  enum Row: Identifiable {
    /// The app's icon, name and whether it is in front.
    case head
    /// One of its windows.
    case window(ViewerWindowRow)
    /// "No windows".
    case noWindows

    /// The row's identity in its group.
    var id: Int {
      switch self {
      case .head: Layout.headID
      case .window(let window): window.id
      case .noWindows: Layout.noWindowsID
      }
    }
  }

  /// The app.
  let app: ViewerAppRow
  /// The window under the pointer, anywhere in the pane; nil for none.
  var hoveredWindow: Int?
  /// Receives each window row the pointer enters or leaves, and which it was.
  var hover: @MainActor (_ window: Int, _ isOver: Bool) -> Void = { _, _ in }
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The app's rows in one group.
  var body: some View {
    GroupedList(rows) { row in
      switch row {
      case .head:
        head
      case .window(let window):
        AppWindowRow(window: window, isHovered: hoveredWindow == window.id) { isOver in
          hover(window.id, isOver)
        } show: {
          send(.showWindow(app: app.id, window: window.id))
        }
      case .noWindows:
        Text(verbatim: "No windows")
          .font(Typography.font(.callout))
          .foregroundStyle(Color.textTertiary)
          .padding(.leading, ViewerLayout.windowIndent)
          .frame(maxWidth: .infinity, minHeight: ViewerLayout.windowRowHeight, alignment: .leading)
          .modifier(WindowRowSeparator())
      }
    }
  }

  /// The group's rows: the head, then the windows or the note that there are none.
  private var rows: [Row] {
    [.head] + (app.windows.isEmpty ? [.noWindows] : app.windows.map(Row.window))
  }

  /// The app's icon, its name and, while it is in front, a tag saying so.
  private var head: some View {
    HStack(spacing: Spacing.sPlus) {
      AppTile(image: app.icon?.image, side: ViewerLayout.appIcon)
      Text(app.name)
        .font(Typography.font(.body, weight: .semibold))
        .lineLimit(1)
      if app.isFront {
        AccentTag(text: "In front")
      }
      Spacer(minLength: .zero)
    }
    .padding(.vertical, Spacing.sPlus)
    .padding(.horizontal, Spacing.m)
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(.isHeader)
  }
}

/// The group's own identities for its rows that are not windows, which a window's number never takes.
private enum Layout {
  /// The head's identity.
  static let headID = -1
  /// The "No windows" row's identity.
  static let noWindowsID = -2
}

#if DEBUG
  #Preview("Samples") {
    if case .apps(let apps) = PreviewStates.viewerApps.pane {
      VStack(spacing: Spacing.sPlus) {
        ForEach(apps.apps) { app in
          AppCard(app: app, hoveredWindow: apps.apps.first?.windows.last?.id) { _ in }
        }
      }
      .frame(width: ViewerLayout.largeShotLargest.width)
      .sampleSurface()
    }
  }
#endif
