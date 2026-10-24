import SwiftUI

/// One of Recent's rows (v3 P5): what was done and, on the right, how long ago in digits that count rather than jump
/// (v3 R11). No mark leads it unless it carries state: a warning sign for what did not work, and a blue dot on the live
/// row that cools away after a second. VoiceOver reads it as one.
struct PanelRecentRow: View {
  /// The action.
  let entry: PanelRecentEntry
  /// Whether the live row's dot has cooled away; it does once per row, a second after the row arrives live.
  @State private var isCooled = false
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @Environment(\.animatesArrivals) private var animatesArrivals

  var body: some View {
    HStack(spacing: .zero) {
      if entry.isFailed {
        ActivityMark(kind: .failed)
          .padding(.trailing, Spacing.s)
      }
      StatusDot(mark: .dot, tone: .active, size: .small)
        .opacity(isGlowing ? 1 : 0)
        .frame(width: isGlowing ? IconSize.xxs : .zero, alignment: .leading)
        .padding(.trailing, isGlowing ? Spacing.s : .zero)
      Text(entry.line)
        .truncationMode(.tail)
        .frame(maxWidth: .infinity, alignment: .leading)
      Text(entry.when)
        .font(Typography.digits(.callout))
        .foregroundStyle(Color.textTertiary)
        .contentTransition(reduceMotion ? .opacity : .numericText())
        .animation(
          Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced),
          value: entry.when
        )
        .padding(.leading, Spacing.s)
    }
    .font(Typography.font(.callout))
    .lineLimit(1)
    .frame(minHeight: PanelLayout.recentRow)
    .padding(.horizontal, Spacing.half)
    .accessibilityElement(children: .combine)
    .onAppear(perform: cool)
    .onChange(of: entry.isLive) { cool() }
  }

  /// Whether the blue dot shows: on the live row until it cools.
  private var isGlowing: Bool {
    entry.isLive && !isCooled
  }

  /// Cools the live row's dot away after ``PanelLayout/liveHold``, through the animation's own delay, never a sleep.
  /// A render draws the row as it arrives, dot and all.
  private func cool() {
    guard entry.isLive, animatesArrivals else { return }
    let cooling = Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced)
      .delay(PanelLayout.liveHold)
    withAnimation(cooling) { isCooled = true }
  }
}

#if DEBUG
  #Preview("Rows") {
    VStack(spacing: .zero) {
      ForEach(PreviewStates.panelWorking.recent) { entry in
        PanelRecentRow(entry: entry)
      }
    }
    .frame(width: PanelLayout.picture.width)
    .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var isLive = true
    @Previewable @State var isFailed = false
    @Previewable @State var attempt = 0
    InteractivePreview {
      Toggle(String("Live"), isOn: $isLive)
      Toggle(String("Failed"), isOn: $isFailed)
      Button(String("Arrive again")) { attempt += 1 }
    } content: {
      PanelRecentRow(
        entry: PanelRecentEntry(
          id: String(attempt),
          line: "Turning Show Center off in Opie",
          when: "\(attempt * 3) s",
          isFailed: isFailed,
          isLive: isLive
        )
      )
      .id(attempt)
      .frame(width: PanelLayout.picture.width)
    }
  }
#endif
