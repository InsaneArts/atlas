import SwiftUI

/// A line's mark: its kind's symbol in a small tile, quiet for an agent's, blue for the person's own, orange
/// for a problem; the live line's tile ringed in blue until the ring cools away, a second after it arrives.
struct ActivityLineMark: View {
  /// The line.
  let line: ViewerActivityLine
  /// Whether the live line's ring has cooled away; it does once per line, a second after the line arrives live.
  @State private var isCooled = false
  @Environment(\.animatesArrivals) private var animatesArrivals

  /// The tile, ringed while the line is live.
  var body: some View {
    SymbolTile(icon: .symbol(line.kind.symbol.rawValue), size: .small, tone: tone)
      .overlay {
        RoundedRectangle(cornerRadius: Layout.ringRadius)
          .strokeBorder(Color.accent, lineWidth: Stroke.emphasis)
          .padding(-Layout.ringReach)
          .opacity(showsRing ? Layout.ringOpacity : 0)
      }
      .onAppear(perform: cool)
      .onChange(of: line.lead) { cool() }
  }

  /// The tile's tone, by how the line went and who acted.
  private var tone: SymbolTile.Tone {
    switch line.lead {
    case .live, .quiet: .quiet
    case .person: .accent
    case .failed: .attention
    }
  }

  /// Whether the blue ring shows: on the live line until it cools.
  private var showsRing: Bool {
    line.lead == .live && !isCooled
  }

  /// Cools the live line's ring away after ``ViewerLayout/liveHold``, through the animation's own delay, never a
  /// sleep: a slow fade on a curve, since a colour change never springs; only opacity changes, so Reduce Motion keeps
  /// it. Renders keep the ring.
  private func cool() {
    guard line.lead == .live, animatesArrivals else { return }
    let cooling = Animation.easeOut(duration: Layout.cooling)
      .delay(ViewerLayout.liveHold)
    withAnimation(cooling) { isCooled = true }
  }
}

/// The mark's own measures for Activity rows.
private enum Layout {
  /// 8: the live ring's corner, the tile's 6 and its 3-point reach, less a little.
  static let ringRadius: CGFloat = 8
  /// 3: how far the live ring stands outside the tile.
  static let ringReach: CGFloat = 3
  /// 0.9: the live ring's opacity before it cools.
  static let ringOpacity = 0.9
  /// 0.4: how long the ring takes to cool away.
  static let cooling = 0.4
}

#if DEBUG
  #Preview("Every lead") {
    if case .activity(let activity) = PreviewStates.viewerActivity.pane {
      HStack(spacing: Spacing.m) {
        ForEach(activity.groups.flatMap(\.lines)) { line in
          ActivityLineMark(line: line)
        }
      }
      .padding()
      .sampleSurface()
    }
  }
#endif
