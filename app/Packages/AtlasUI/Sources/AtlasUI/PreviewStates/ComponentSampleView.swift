#if DEBUG
  import SwiftUI

  /// One component's samples, as its preview draws them.
  public struct ComponentSampleView: View {
    /// Which component.
    private let sample: ComponentSample

    /// Creates the samples of one component.
    ///
    /// - Parameter sample: Which component.
    public init(_ sample: ComponentSample) {
      self.sample = sample
    }

    /// The chosen component's samples.
    public var body: some View {
      switch sample {
      case .capsuleButtons: CapsuleButtonSamples(.every)
      case .statusDots: IndicatorSamples(.statusDot)
      case .floatingChip: FloatingChipSamples()
      case .alertCard: AlertCardSamples()
      case .emptyMessage: EmptyMessageSamples()
      case .pictureFrame: PictureFrameSamples()
      case .miniScreen: MiniScreenSamples()
      case .searchField: SearchFieldSamples()
      case .groupLabel: GroupLabelSamples()
      case .popupButton: PopupButtonSamples()
      case .appIconTiles: BrandSamples(.appIcon)
      case .appGlyph: AppGlyphSamples(.beside)
      case .monogramTints: AppGlyphSamples(.tints)
      case .tagTones: IndicatorSamples(.tag)
      case .activityGlyphs: AppGlyphSamples(.activity)
      case .keycapsSoft: KeycapSamples()
      case .appCards: AppsSamples(.cards)
      case .activitySteps: ActivitySamples(.steps)
      case .activityMoments: ActivitySamples(.moments)
      case .accountPictureSizes: BrandSamples(.accountPicture)
      case .listRowChoices: ListRowSamples(.choices)
      case .listRowLeads: ListRowSamples(.leads)
      case .listRowTrailing: ListRowSamples(.trailing)
      case .listRowSizes: ListRowSamples(.sizes)
      case .groupedListScrolling: ListRowSamples(.scrolling)
      case .statusLineKinds: StatusLineSamples(.kinds)
      case .progressTrackSteps: IndicatorSamples(.progressTrack)
      case .heroDiscKinds: IndicatorSamples(.heroDisc)
      case .pictureChipKinds: PictureChipSamples(.chips)
      case .capsuleButtonCircles: CapsuleButtonSamples(.circles)
      case .actionSlotActions: ActionSlotSamples(isInControl: true)
      case .symbolTileSizes: IndicatorSamples(.symbolTile)
      case .symbolNames: SymbolSamples()
      case .tabs: TabsSamples()
      case .groupedSection: GroupedSectionSamples()
      case .statusPill: IndicatorSamples(.statusPill)
      case .lineSwapLines: LineSwapSamples()
      case .stateBadgeStates: StateBadgeSamples()
      }
    }
  }
#endif
