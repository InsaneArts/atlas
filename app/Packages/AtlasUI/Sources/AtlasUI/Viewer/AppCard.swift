import SwiftUI

/// An app open in the helper, on its own card: its icon, its name and whether it is in front, then its windows front
/// to back, or "No windows".
struct AppCard: View {
  /// The app.
  let app: ViewerAppRow
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  var body: some View {
    VStack(spacing: .zero) {
      HStack(spacing: Spacing.m) {
        AppTile(image: app.icon?.image)
        Text(app.name)
          .font(Typography.font(.headline, weight: .semibold))
          .lineLimit(1)
        if app.isFront {
          AccentTag(text: "In front")
        }
        Spacer(minLength: .zero)
      }
      .padding(.leading, Spacing.mPlus)
      .padding(.trailing, Spacing.l)
      .frame(minHeight: ViewerLayout.appRowHeight)
      .accessibilityElement(children: .combine)
      .accessibilityAddTraits(.isHeader)
      windows
    }
    .cardSurface()
  }

  /// The app's windows, or "No windows".
  @ViewBuilder private var windows: some View {
    if app.windows.isEmpty {
      Text("No windows")
        .font(Typography.font(.callout))
        .foregroundStyle(Color.textTertiary)
        .padding(.leading, ViewerLayout.windowIndent)
        .frame(maxWidth: .infinity, minHeight: ViewerLayout.noWindowsHeight, alignment: .leading)
        .overlay(alignment: .top) {
          SeparatorLine()
        }
    } else {
      ForEach(app.windows) { window in
        AppWindowRow(window: window) { send(.showWindow(app: app.id, window: window.id)) }
      }
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    if case .apps(let apps) = PreviewStates.viewerApps.pane {
      VStack(spacing: Spacing.sPlus) {
        ForEach(apps.apps) { app in
          AppCard(app: app) { _ in }
        }
      }
      .frame(width: ViewerLayout.largeShotLargest.width)
      .sampleSurface()
    }
  }
#endif
