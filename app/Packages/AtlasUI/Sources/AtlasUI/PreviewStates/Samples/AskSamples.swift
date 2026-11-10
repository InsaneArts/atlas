#if DEBUG
  import SwiftUI

  /// The samples of asking before an agent sends: the panel's cards, the viewer's banner and an account's level.
  struct AskSamples: View {
    /// Which samples.
    enum Kind: Sendable {
      /// The panel with each question, and with what the person allowed.
      case panel
      /// The banner over a picture, wide and narrow.
      case banner
      /// The level control at each level, and the popup.
      case levels
    }

    /// Which samples.
    let kind: Kind

    /// Creates the samples.
    ///
    /// - Parameter kind: Which samples.
    init(_ kind: Kind) {
      self.kind = kind
    }

    /// The samples, on the ground.
    var body: some View {
      switch kind {
      case .panel: panels
      case .banner: banners
      case .levels: levels
      }
    }

    /// The panel in each asking state.
    private var panels: some View {
      HStack(alignment: .top, spacing: Spacing.xxl) {
        ForEach(Array(Self.panelStates.enumerated()), id: \.offset) { _, state in
          PanelView(state: state) { _ in }
            .panelCard()
        }
      }
      .padding(Spacing.xxxl)
      .sampleSurface()
    }

    /// The panels drawn.
    @MainActor static let panelStates = [
      PreviewStates.panelAskTyped,
      PreviewStates.panelAskNamedClick,
      PreviewStates.panelAskUnnamedClick,
      PreviewStates.panelAskPassword,
      PreviewStates.panelAskTaskOnly,
      PreviewStates.panelAllowed,
    ]

    /// The banner over the sample screen, wide and narrow.
    private var banners: some View {
      VStack(spacing: Spacing.xl) {
        ForEach(Self.bannerWidths, id: \.self) { width in
          banner(width: width)
        }
      }
      .padding(Spacing.xl)
      .sampleSurface()
    }

    /// The banner over the sample screen at a width.
    ///
    /// - Parameter width: The screen's width.
    /// - Returns: The sample.
    private func banner(width: CGFloat) -> some View {
      Color.letterbox
        .overlay {
          if let picture = SampleDesktop.picture {
            Image(decorative: picture.image, scale: 1)
              .resizable()
          }
        }
        .overlay(alignment: .bottom) {
          if let ask = PreviewStates.viewerAsk.screen.ask {
            ScreenAskBanner(ask: ask) { _ in }
              .padding(AskBannerLayout.edge)
          }
        }
        .frame(width: width, height: width * Self.screenShape)
        .clipShape(RoundedRectangle(cornerRadius: Radius.large, style: .continuous))
    }

    /// The screens' widths the banner is drawn over: a wide window's, where it stops at its widest, and a narrow one's,
    /// where its answers go under its words.
    static let bannerWidths: [CGFloat] = [900, 440]
    /// The sample screen's height over its width, 900 by 1440.
    static let screenShape: CGFloat = 0.625

    /// The level tiles at each level, and with none known.
    private var levels: some View {
      VStack(alignment: .leading, spacing: Spacing.l) {
        ForEach(AccountLevelChoice.allCases, id: \.self) { level in
          AccountLevelPicker(account: "github.com", level: level) { _ in }
        }
        AccountLevelPicker(account: "github.com", level: nil) { _ in }
      }
      .frame(width: SampleWidth.form)
      .padding(Spacing.xl)
      .sampleSurface()
    }
  }
#endif
