import SwiftUI

/// The latest actions as one pile of glass chips in the screen's corner (v3 V6): the newest in front with a breathing
/// blue dot, at most two older ones behind it, each a step higher, smaller and fainter, peeking over its top. The
/// pointer fans them out. A new action pushes in at the front with a blur; the oldest folds away. A failure keeps its
/// orange dot wherever it stands. Under Reduce Motion the chips fade in and out and the fan opens at once.
struct CaptionStack: View {
  /// The actions, oldest first; the newest few show.
  let entries: [ActivityEntry]
  /// Whether the pointer is over the pile, which fans it out.
  @State private var isFanned = false
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The newest chips, piled or fanned out.
  var body: some View {
    let shown = Self.shown(entries)
    ZStack(alignment: .bottomLeading) {
      ForEach(shown) { item in
        chip(item.entry)
          // Where a chip stands moves on the state spring, at once under Reduce Motion; how faint it is fades either
          // way. Each is scoped to its own effect, so nothing moves while a new action arrives under Reduce Motion.
          .animation(Self.moving(reduceMotion: reduceMotion)) { chip in
            chip
              .scaleEffect(Layout.scale(depth: item.depth, fanned: isFanned), anchor: .bottomLeading)
              .offset(y: Layout.offset(depth: item.depth, fanned: isFanned))
          }
          .animation(Self.fading(reduceMotion: reduceMotion)) { chip in
            chip
              .opacity(Layout.opacity(depth: item.depth, fanned: isFanned))
          }
          .zIndex(-Double(item.depth))
          .transition(arrival)
      }
    }
    // The pile reaches as high as its fan, so the pointer keeps it open while it moves onto an older chip.
    .frame(height: Layout.height(count: shown.count, fanned: isFanned), alignment: .bottomLeading)
    .contentShape(Rectangle())
    .onHover { isFanned = $0 }
    .animation(Self.moving(reduceMotion: reduceMotion), value: isFanned)
    .animation(Self.moving(reduceMotion: reduceMotion), value: shown.map(\.id))
    .accessibilityElement(children: .contain)
    .accessibilityLabel(Text(verbatim: "Latest actions"))
  }

  /// One action's chip: its mark, what was done, and how long ago in tertiary digits.
  ///
  /// - Parameter entry: The action.
  /// - Returns: The chip.
  private func chip(_ entry: ActivityEntry) -> some View {
    GlassChip(lead: Self.lead(entry)) {
      Text(entry.line)
      Text(entry.when)
        .font(Typography.digits(.callout))
        .foregroundStyle(Color.textTertiary)
        .contentTransition(.numericText())
    }
  }

  /// How a chip comes and goes: in at the front with a blur, the oldest fading as it folds away; a fade under Reduce
  /// Motion, on its own short animation, since nothing else moves then.
  private var arrival: AnyTransition {
    guard !reduceMotion else {
      return AnyTransition.opacity
        .animation(Self.fading(reduceMotion: reduceMotion))
    }
    return .asymmetric(
      insertion: AnyTransition(.blurReplace(.downUp))
        .animation(Motion.arrive),
      removal: AnyTransition.opacity
        .animation(Motion.leave)
    )
  }

  /// How the chips move to new places, as an action arrives or the pile fans out: on the state spring, or at once under
  /// Reduce Motion.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation, or nil.
  nonisolated static func moving(reduceMotion: Bool) -> Animation? {
    Motion.animation(Motion.state, reduceMotion: reduceMotion)
  }

  /// How the chips fade to their new faintness: on the state spring, or a plain fade under Reduce Motion.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation.
  nonisolated static func fading(reduceMotion: Bool) -> Animation {
    Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced)
  }

  /// The newest actions the pile shows, each with its depth: at most ``Layout/limit``, the newest at depth 0, in
  /// front.
  ///
  /// - Parameter entries: The actions, oldest first.
  /// - Returns: The shown actions, oldest first, with their depths.
  nonisolated static func shown(_ entries: [ActivityEntry]) -> [Shown] {
    let newest = entries.suffix(Layout.limit)
    return newest.enumerated()
      .map { index, entry in Shown(entry: entry, depth: newest.count - 1 - index) }
  }

  /// What leads a chip: an orange dot for a failure, a breathing blue dot for the newest, nothing otherwise.
  ///
  /// - Parameter entry: The action.
  /// - Returns: The chip's lead.
  nonisolated static func lead<Content: View>(_ entry: ActivityEntry) -> GlassChip<Content>.Lead {
    switch entry.captionMark {
    case .failed: .dot(.attention)
    case .newest: .pulsing(.active)
    case nil: .none
    }
  }
}

extension CaptionStack {
  /// An action the pile shows, and how many chips stand in front of it.
  struct Shown: Identifiable {
    /// The action.
    let entry: ActivityEntry
    /// How many chips stand in front of it; 0 for the newest.
    let depth: Int

    /// The action's id, which holds as newer actions arrive.
    var id: String { entry.id }
  }

  /// The pile's geometry (v3 V6, `.vtrail`).
  enum Layout {
    /// 3: the most chips the pile shows.
    static let limit = 3
    /// 7: how far each chip behind peeks over the one in front of it.
    static let peek: CGFloat = 7
    /// 34: how far apart the chips stand once fanned out, a chip's 28 and a 6-point gap.
    static let fan: CGFloat = 34
    /// 0.05: how much smaller each step back draws.
    static let shrink: CGFloat = 0.05
    /// 0.35: how much fainter each step back draws.
    static let fade = 0.35

    /// How far up a chip sits.
    ///
    /// - Parameters:
    ///   - depth: How many chips stand in front of it; 0 for the newest.
    ///   - fanned: Whether the pile is fanned out.
    /// - Returns: The vertical offset, negative upward.
    static func offset(depth: Int, fanned: Bool) -> CGFloat {
      -(fanned ? fan : peek) * CGFloat(depth)
    }

    /// How large a chip draws: smaller each step back, whole once fanned.
    ///
    /// - Parameters:
    ///   - depth: How many chips stand in front of it.
    ///   - fanned: Whether the pile is fanned out.
    /// - Returns: The scale.
    static func scale(depth: Int, fanned: Bool) -> CGFloat {
      fanned ? 1 : 1 - shrink * CGFloat(depth)
    }

    /// How opaque a chip draws: fainter each step back, whole once fanned.
    ///
    /// - Parameters:
    ///   - depth: How many chips stand in front of it.
    ///   - fanned: Whether the pile is fanned out.
    /// - Returns: The opacity.
    static func opacity(depth: Int, fanned: Bool) -> Double {
      fanned ? 1 : 1 - fade * Double(depth)
    }

    /// How tall the pile stands: the front chip and the reach of the highest one behind it.
    ///
    /// - Parameters:
    ///   - count: How many chips show.
    ///   - fanned: Whether the pile is fanned out.
    /// - Returns: The height.
    static func height(count: Int, fanned: Bool) -> CGFloat {
      guard count > 0 else { return .zero }
      return ControlHeight.s - offset(depth: count - 1, fanned: fanned)
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    CaptionStackSamples()
  }

  #Preview("Interactive") {
    @Previewable @State var count = 3
    InteractivePreview {
      Button(String("New action")) { count += 1 }
    } content: {
      CaptionStack(entries: CaptionStackSamples.entries(count: count))
        .padding(Spacing.xxl)
        .sampleSurface()
    }
  }
#endif
