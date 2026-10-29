#if DEBUG
  import SwiftUI

  /// The state badge's samples: a session icon badged acting, waiting and needing the person, then one with no badge,
  /// each named, on the recessed ground the sessions sit on.
  struct StateBadgeSamples: View {
    /// 5: how far the badge's ground ring reaches past the icon's corner, as the sessions draw it.
    private static let reach: CGFloat = 5

    /// The states shown, in order; nil is an icon with no badge.
    private let badges: [SessionBadge?]

    /// Creates the samples.
    ///
    /// - Parameter badges: The states shown, in order; every state and none by default.
    init(_ badges: [SessionBadge?] = SessionBadge.allCases + [nil]) {
      self.badges = badges
    }

    /// The badged icons in a row, each over its state's name.
    var body: some View {
      HStack(spacing: Spacing.xl) {
        ForEach(badges.indices, id: \.self) { index in
          VStack(spacing: Spacing.s) {
            SessionIcon(icon: SessionSamples.claudeIcon, side: SessionIcon.rowSide)
              .overlay(alignment: .bottomTrailing) {
                StateBadge(badge: badges[index])
                  .offset(x: Self.reach, y: Self.reach)
              }
            Text(verbatim: Self.name(of: badges[index]))
              .textStyle(.caption)
              .foregroundStyle(Color.textSecondary)
          }
        }
      }
      .padding(Spacing.l)
      .background(Color.groundRecessed)
      .sampleSurface()
    }

    /// A state's name under its sample.
    ///
    /// - Parameter badge: The state, or nil for none.
    /// - Returns: The name.
    private static func name(of badge: SessionBadge?) -> String {
      switch badge {
      case .acting: "Acting"
      case .waiting: "Waiting"
      case .needsYou: "Needs you"
      case nil: "None"
      }
    }
  }
#endif
