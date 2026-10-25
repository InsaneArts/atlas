import SwiftUI

/// One of Recent's rows: what was done and, on the right, how long ago, whose digits count rather than jump while the
/// unit stays, and which swaps by opacity when the words change, as from "now" to "1 s" or from seconds to minutes. No
/// mark leads it unless it carries state: a warning sign for what did not work, and a blue dot on the live row that
/// cools away after a second. Under Reduce Motion the dot only fades as it cools: its place stays while the
/// row is live, so the words never move. VoiceOver reads it as one.
struct PanelRecentRow: View {
  /// The action.
  let entry: PanelRecentEntry
  /// Whether the live row's dot has cooled away; it does once per row, a second after the row arrives live.
  @State private var isCooled = false
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @Environment(\.animatesArrivals) private var animatesArrivals

  /// Its mark if it carries state, what was done, and how long ago.
  var body: some View {
    HStack(spacing: .zero) {
      if entry.isFailed {
        ActivityMark(kind: .failed)
          .padding(.trailing, Spacing.s)
      }
      StatusDot(mark: .dot, tone: .active, size: .small)
        .opacity(isGlowing ? 1 : 0)
        .frame(width: holdsDot ? IconSize.xxs : .zero, alignment: .leading)
        .padding(.trailing, holdsDot ? Spacing.s : .zero)
      Text(entry.line)
        .truncationMode(.tail)
        .frame(maxWidth: .infinity, alignment: .leading)
      ZStack(alignment: .trailing) {
        Text(entry.when)
          .font(Typography.digits(.callout))
          .foregroundStyle(Color.textTertiary)
          .contentTransition(Motion.contentTransition(.numericText(), reduceMotion: reduceMotion))
          // A new unit, or words in place of digits, is a new text, which crossfades instead of rolling.
          .id(Self.unit(of: entry.when))
          .transition(.opacity)
      }
      .animation(Motion.textSwap, value: entry.when)
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

  /// Whether the dot keeps its place before the words.
  private var holdsDot: Bool {
    Self.holdsDot(isLive: entry.isLive, isCooled: isCooled, reduceMotion: reduceMotion)
  }

  /// Whether the dot keeps its place before the words: while it glows, so the words close the gap as it cools away;
  /// under Reduce Motion for as long as the row is live, so cooling only fades the dot and nothing moves.
  ///
  /// - Parameters:
  ///   - isLive: Whether the row is live.
  ///   - isCooled: Whether its dot has cooled away.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: Whether the dot's place stays.
  nonisolated static func holdsDot(isLive: Bool, isCooled: Bool, reduceMotion: Bool) -> Bool {
    if reduceMotion { return isLive }
    return isLive && !isCooled
  }

  /// What a time's digits count in: its unit when it is a number and a unit, such as "s" for "9 s"; else the words
  /// themselves, such as "now", so a change between them, or between units, is a new text.
  ///
  /// - Parameter time: How long ago, such as "9 s" or "now".
  /// - Returns: The unit, or the whole words.
  nonisolated static func unit(of time: String) -> String {
    let parts = time.split(separator: " ", maxSplits: 1)
    guard parts.count == 2, parts[0].allSatisfy(\.isNumber) else { return time }
    return String(parts[1])
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
