#if DEBUG
  import SwiftUI

  /// The app glyph's samples: the drawn tile in every tint at the sides the lists use, each over its app's name,
  /// drawn tiles in a row with real apps' icons at 26 and 44, so a tile can be seen to sit as an icon does, and what
  /// leads an action at an Activity moment's 26 and a Recent row's 20: an app, Atlas, the person and commands.
  struct AppGlyphSamples: View {
    /// Which samples show.
    enum Kind: Sendable {
      /// A name of each tint, as tiles at 44, 26 and 16 over the name.
      case tints
      /// Tiles between real icons, at 26 and 44.
      case beside
      /// What leads an action, at 26 and 20.
      case activity
    }

    /// One name of each tint, in the tints' order: Juniper is indigo, through Nimbus in graphite.
    static let tintNames = ["Juniper", "Probe", "Relay", "Harbor", "Ledger", "Tally", "Quill", "Nimbus"]
    /// The names drawn between the real icons, a digit's among them.
    static let besideNames = ["Probe", "1Password", "Ledger", "Quill"]
    /// The sides the tiles are drawn at, largest first: a card's, a row's and a chip's.
    static let sides: [CGFloat] = [44, 26, 16]
    /// The sides the tiles stand beside real icons at.
    static let besideSides: [CGFloat] = [44, 26]
    /// The sides what leads an action is drawn at: a moment's and a Recent row's.
    static let activitySides: [CGFloat] = [26, 20]

    /// Which samples show.
    private let kind: Kind
    /// Real apps' icons, when the render read them.
    @Environment(\.sampleAppIcons) private var realIcons

    /// Creates the samples.
    ///
    /// - Parameter kind: Which samples show.
    init(_ kind: Kind) {
      self.kind = kind
    }

    /// The samples, on the ground.
    var body: some View {
      Group {
        switch kind {
        case .tints: tints
        case .beside: beside
        case .activity: activity
        }
      }
      .sampleSurface()
    }

    /// Each tint's tile at every side, over its name.
    private var tints: some View {
      HStack(alignment: .top, spacing: Spacing.l) {
        ForEach(Self.tintNames, id: \.self) { name in
          VStack(spacing: Spacing.s) {
            ForEach(Self.sides, id: \.self) { side in
              AppGlyph(face: .monogram(AppMonogram(name: name)), side: side)
            }
            Text(verbatim: name)
              .textStyle(.caption)
              .foregroundStyle(Color.textSecondary)
          }
        }
      }
    }

    /// Real icons and tiles taking turns, a row at each side.
    private var beside: some View {
      VStack(alignment: .leading, spacing: Spacing.m) {
        ForEach(Self.besideSides, id: \.self) { side in
          HStack(spacing: Spacing.m) {
            ForEach(faces.indices, id: \.self) { index in
              AppGlyph(face: faces[index], side: side)
            }
          }
        }
      }
    }

    /// What leads an action, a row at each side: an app's icon, an app's tile, Atlas, the person, commands and
    /// screenshots.
    private var activity: some View {
      VStack(alignment: .leading, spacing: Spacing.m) {
        ForEach(Self.activitySides, id: \.self) { side in
          HStack(spacing: Spacing.m) {
            ForEach(activityIcons.indices, id: \.self) { index in
              ActivityGlyph(icon: activityIcons[index], side: side)
            }
          }
        }
      }
    }

    /// What leads an action, one of each.
    private var activityIcons: [ActivityIcon] {
      let app = (realIcons.first ?? SampleAppIcons.icons[SampleAppIcons.notes])
        .map { ActivityIcon.app(.image($0)) }
      return [app, .app(.monogram(AppMonogram(name: "Probe"))), .atlas, .you, .symbol(.terminal), .symbol(.camera)]
        .compactMap { $0 }
    }

    /// The real icons, or the stand-ins, with a tile after each.
    private var faces: [AppIconFace] {
      let standIns = SampleAppIcons.icons.values
        .sorted { $0.id < $1.id }
      let icons = realIcons.isEmpty ? standIns : realIcons
      return zip(icons, Self.besideNames)
        .flatMap { icon, name in [AppIconFace.image(icon), .monogram(AppMonogram(name: name))] }
    }
  }
#endif
