import SwiftUI

/// The top row's sessions button, beside the status: the live sessions' icons, stacked just over each other, and
/// "All sessions", or the chosen session's icon and name, then a chevron, on a small glass capsule, pressed while its
/// menu is open. In the bar's narrower layouts it keeps its icons and chevron alone. It is no taller than the bar's
/// other buttons, so the bar keeps its height. It tells the window where it is as that changes, so the window can open
/// its menu under it by itself, as the panel's row for more sessions asks.
struct ViewerSessionsPicker: View {
  /// What the button shows.
  let button: ViewerSessionsButton
  /// Whether it keeps its words.
  let showsWords: Bool
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// Where the button is, in the window, so its menu opens under it.
  @State private var frame = CGRect.zero

  /// The icons, the words and the chevron.
  var body: some View {
    Button {
      send(.showSessions(below: frame))
    } label: {
      HStack(spacing: Spacing.xsPlus) {
        HStack(spacing: ViewerLayout.sessionStackSpacing) {
          ForEach(Array(button.icons.enumerated()), id: \.offset) { _, icon in
            SessionIcon(icon: icon, side: ViewerLayout.sessionIcon)
          }
        }
        if showsWords {
          Text(button.title)
            .lineLimit(1)
            .truncationMode(.middle)
        }
        Image(systemName: SymbolName.menu.rawValue)
          .font(Typography.symbol(.row, weight: .semibold))
          .foregroundStyle(Color.textSecondary)
      }
    }
    .buttonStyle(.capsule(.glass, size: .small, isOn: button.isOpen))
    .fixedSize()
    .onGeometryChange(for: CGRect.self) {
      $0.frame(in: .global)
    } action: {
      frame = $0
      send(.placeSessions($0))
    }
    .onDisappear { send(.placeSessions(nil)) }
    .help(button.help)
    .accessibilityLabel(button.title)
    .accessibilityHint(Text(verbatim: "Opens a menu"))
  }
}

#if DEBUG
  #Preview("All sessions") {
    if let sessions = PreviewStates.viewerSessions.top.sessions {
      ViewerSessionsPicker(button: sessions, showsWords: true) { _ in }
        .sampleSurface()
    }
  }

  #Preview("One session chosen") {
    if let sessions = PreviewStates.viewerActivityOneSession.top.sessions {
      VStack(spacing: Spacing.m) {
        ViewerSessionsPicker(button: sessions, showsWords: true) { _ in }
        ViewerSessionsPicker(button: sessions, showsWords: false) { _ in }
      }
      .sampleSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var showsWords = true
    @Previewable @State var isOpen = false
    if let sessions = PreviewStates.viewerSessions.top.sessions {
      InteractivePreview {
        Toggle(String("Words"), isOn: $showsWords)
      } content: {
        ViewerSessionsPicker(
          button: {
            var shown = sessions
            shown.isOpen = isOpen
            return shown
          }(),
          showsWords: showsWords
        ) { event in
          if case .showSessions = event { isOpen.toggle() }
        }
      }
    }
  }
#endif
