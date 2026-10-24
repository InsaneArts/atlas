import SwiftUI

/// The panel's header, which leads with the status: its mark and word as the heading, in the word's colour, the line
/// under them, and ⋯ at the trailing edge. A new word or line swaps in place of the old by opacity, the same under
/// Reduce Motion. VoiceOver reads the status as one, then the line.
struct PanelHeader: View {
  /// The helper's status, with its line.
  let status: StatusSummary
  /// What the panel's last action said when it did not work, in place of the line; nil shows the line.
  let notice: String?
  /// Whether the ⋯ menu is open, which draws its button pressed.
  let isMoreOpen: Bool
  /// Receives what the person does.
  let send: @MainActor (PanelEvent) -> Void
  /// Where ⋯ is in the panel, so its menu opens under it.
  @State private var moreFrame = CGRect.zero

  /// The status's word and line on the leading edge, ⋯ on the trailing one.
  var body: some View {
    HStack(alignment: .top, spacing: Spacing.sPlus) {
      VStack(alignment: .leading, spacing: Spacing.half) {
        heading
          .animation(PanelLayout.change, value: status.label)
        lineRow
          .animation(PanelLayout.change, value: line)
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      Button {
        send(.showMore(below: moreFrame))
      } label: {
        Image(systemName: SymbolName.more.rawValue)
      }
      .buttonStyle(.capsule(.glass, size: .mini, outline: .circle, isOn: isMoreOpen))
      .onGeometryChange(for: CGRect.self) {
        $0.frame(in: .global)
      } action: {
        moreFrame = $0
      }
      .help("More")
      .accessibilityLabel("More")
    }
    .padding(.horizontal, Spacing.half)
  }

  /// The status's mark and word, the word in its tone's colour.
  private var heading: some View {
    HStack(spacing: PanelLayout.wordGap) {
      StatusDot(mark: status.mark, tone: status.tone, size: .large)
      Text(status.label)
        .textStyle(.heading)
        .foregroundStyle(status.tone.wordColor)
        .lineLimit(1)
        .swapsText(on: status.label)
    }
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(.updatesFrequently)
  }

  /// The line, or the notice in its place in orange, in one line cut short with an ellipsis; nothing when both are
  /// empty.
  @ViewBuilder private var lineRow: some View {
    if !line.isEmpty {
      Text(line)
        .textStyle(.callout)
        .foregroundStyle(notice == nil ? Color.textSecondary : Color.textAttention)
        .lineLimit(1)
        .truncationMode(.tail)
        .swapsText(on: line)
    }
  }

  /// The words under the heading: the notice when there is one, else the status's line.
  private var line: String {
    notice ?? status.line
  }
}

#if DEBUG
  #Preview("Idle") {
    PanelHeader(status: PreviewStates.panelIdle.status, notice: nil, isMoreOpen: false) { _ in }
      .frame(width: PanelLayout.picture.width)
      .sampleSurface()
  }

  #Preview("Stop refused") {
    PanelHeader(
      status: PreviewStates.panelStopRefused.status,
      notice: PreviewStates.panelStopRefused.notice,
      isMoreOpen: false
    ) { _ in }
    .frame(width: PanelLayout.picture.width)
    .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.panelIdle
    InteractivePreview {
      Menu(String("State")) {
        ForEach(PreviewStates.panel) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
    } content: {
      PanelHeader(status: state.status, notice: state.notice, isMoreOpen: state.isMoreOpen) { event in
        if case .showMore = event { state.isMoreOpen.toggle() }
      }
      .frame(width: PanelLayout.picture.width)
    }
  }
#endif
